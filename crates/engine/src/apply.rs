//! The command pipeline: validate -> apply -> emit events.
//!
//! `apply` works on an owned clone of the caller's state; rejected commands
//! therefore never leak partial mutations. All movement, payment, and
//! bankruptcy logic lives here; content lookups go through the registries
//! and rule fragments go through the injected strategies.

use crate::command::{CommandKind, PlayerCommand};
use crate::content::{CardEffect, GameContent, MarketEffect, PropertyDef, RentModel, TileKind};
use crate::error::CommandError;
use crate::event::{DeckKind, Event};
use crate::state::{ActiveMarketEvent, GamePhase, GameState, TradeOffer, TurnPhase};
use crate::{Engine, Strategies};

/// Card chains ("advance to X" landing on another card tile) are bounded to
/// keep resolution finite regardless of mod content.
const MAX_CARD_CHAIN_DEPTH: u8 = 4;
/// Anti-spam cap on standing offers per proposer.
const MAX_OPEN_TRADES_PER_PLAYER: usize = 4;
/// Rent-boost cap and per-step rent increase (ADR-0012).
const MAX_RENT_BOOSTS: u8 = 3;
const RENT_BOOST_STEP_PCT: i64 = 50;

pub(crate) fn apply(
    engine: &Engine,
    state: &GameState,
    cmd: &PlayerCommand,
) -> Result<(GameState, Vec<Event>), CommandError> {
    if matches!(state.phase, GamePhase::Finished { .. }) {
        return Err(CommandError::GameFinished);
    }
    let player = state
        .players
        .iter()
        .position(|p| p.id == cmd.player)
        .ok_or(CommandError::UnknownPlayer)?;
    if state.players[player].bankrupt {
        return Err(CommandError::Bankrupt);
    }
    let any_turn = matches!(
        cmd.kind,
        CommandKind::Resign
            | CommandKind::ProposeTrade { .. }
            | CommandKind::AcceptTrade { .. }
            | CommandKind::DeclineTrade { .. }
            | CommandKind::CancelTrade { .. }
    );
    // A sealed-bid window (ADR-0018) has no single actor: any living seat
    // may submit while it's open, regardless of whose turn it nominally is.
    let in_open_bid = matches!(cmd.kind, CommandKind::SubmitBlindBid { .. })
        && matches!(state.turn, TurnPhase::BlindAuction { .. });
    if !any_turn && !in_open_bid && player != state.current {
        return Err(CommandError::NotYourTurn);
    }

    let mut exec = Exec {
        content: engine.content(),
        strat: engine.strategies(),
        st: state.clone(),
        ev: Vec::new(),
    };

    match &cmd.kind {
        CommandKind::Roll => exec.roll(player)?,
        CommandKind::SubmitBlindBid { amount } => exec.submit_blind_bid(player, *amount)?,
        CommandKind::ProposeTrade {
            to,
            give_cash,
            give_tiles,
            receive_cash,
            receive_tiles,
        } => exec.propose_trade(
            player,
            to,
            *give_cash,
            give_tiles,
            *receive_cash,
            receive_tiles,
        )?,
        CommandKind::AcceptTrade { trade } => exec.accept_trade(player, *trade)?,
        CommandKind::DeclineTrade { trade } => exec.decline_trade(player, *trade)?,
        CommandKind::CancelTrade { trade } => exec.cancel_trade(player, *trade)?,
        CommandKind::Build { tile } => exec.build(player, tile)?,
        CommandKind::SellHouse { tile } => exec.sell_house(player, tile)?,
        CommandKind::Expropriate { tile } => exec.expropriate(player, tile)?,
        CommandKind::BoostRent { tile } => exec.boost_rent(player, tile)?,
        CommandKind::Mortgage { tile } => exec.mortgage(player, tile)?,
        CommandKind::Unmortgage { tile } => exec.unmortgage(player, tile)?,
        CommandKind::PayJailFine => exec.pay_jail_fine(player)?,
        CommandKind::UseJailCard => exec.use_jail_card(player)?,
        CommandKind::EndTurn => exec.end_turn(player)?,
        CommandKind::Resign => exec.resign(player)?,
    }

    // A player can go bankrupt during their own turn (jail fine, card debt).
    // The turn must then move on without requiring further input from them -
    // but not while a sealed-bid window is still open (ADR-0018): other
    // seats may still need to bid, and advancing here would wipe out an
    // in-progress window out from under them. This fires correctly on the
    // next command once resolution moves `turn` off `BlindAuction`.
    if matches!(exec.st.phase, GamePhase::Active)
        && exec.st.players[exec.st.current].bankrupt
        && !matches!(exec.st.turn, TurnPhase::BlindAuction { .. })
    {
        exec.advance_turn();
    }

    // Instant win by controlling enough full groups (ADR-0013), checked
    // after any holdings-changing command.
    exec.check_group_win();

    Ok((exec.st, exec.ev))
}

struct Exec<'e> {
    content: &'e GameContent,
    strat: Strategies<'e>,
    st: GameState,
    ev: Vec<Event>,
}

impl<'e> Exec<'e> {
    // -- Commands -------------------------------------------------------------

    fn roll(&mut self, p: usize) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitRoll {
            return Err(CommandError::WrongPhase);
        }
        let (d1, d2) = self.strat.dice.roll(&mut self.st.rng);
        self.ev.push(Event::DiceRolled { player: p, d1, d2 });
        let total = (d1 + d2) as usize;

        if self.st.players[p].jail_turns.is_some() {
            self.roll_from_jail(p, d1, d2, total);
            return Ok(());
        }

        if d1 == d2 {
            self.st.players[p].doubles_streak += 1;
            if self.st.players[p].doubles_streak == 3 {
                self.go_to_jail(p);
                self.st.turn = TurnPhase::AwaitEnd;
                return Ok(());
            }
        } else {
            self.st.players[p].doubles_streak = 0;
        }

        self.move_forward(p, total);
        self.resolve_landing(p, total as u8, 0);
        Ok(())
    }

    fn roll_from_jail(&mut self, p: usize, d1: u8, d2: u8, total: usize) {
        if d1 == d2 {
            self.st.players[p].jail_turns = None;
            self.st.players[p].doubles_streak = 0; // no bonus roll after a jail escape
            self.ev.push(Event::LeftJail { player: p });
            self.move_forward(p, total);
            self.resolve_landing(p, total as u8, 0);
            return;
        }
        let turns = self.st.players[p].jail_turns.map(|t| t + 1).unwrap_or(1);
        self.st.players[p].jail_turns = Some(turns);
        if turns < 3 {
            self.st.turn = TurnPhase::AwaitEnd;
            return;
        }
        // Third failed escape: a held card is spent instead of the fine
        // (strictly better for the player: cards have no other use), else
        // the fine is due. Either way the player then moves.
        if self.st.players[p].jail_cards > 0 {
            self.st.players[p].jail_cards -= 1;
            self.ev.push(Event::JailCardUsed { player: p });
        } else {
            let fine = self.content.rules.jail_fine;
            self.ev.push(Event::JailFinePaid {
                player: p,
                amount: fine,
            });
            self.charge(p, None, fine);
            if self.st.players[p].bankrupt {
                return;
            }
        }
        self.st.players[p].jail_turns = None;
        self.ev.push(Event::LeftJail { player: p });
        self.move_forward(p, total);
        self.resolve_landing(p, total as u8, 0);
    }

    // -- Trading ----------------------------------------------------------------
    //
    // Trades are asynchronous: any solvent player may propose or respond at
    // any time, except during an auction (accepting there would move cash
    // and break the auction's "winner can pay" invariant).

    fn propose_trade(
        &mut self,
        p: usize,
        to_id: &str,
        give_cash: i64,
        give_tiles: &[String],
        receive_cash: i64,
        receive_tiles: &[String],
    ) -> Result<(), CommandError> {
        self.reject_during_auction()?;
        let to = self
            .st
            .players
            .iter()
            .position(|pl| pl.id == to_id)
            .ok_or(CommandError::UnknownPlayer)?;
        if to == p || self.st.players[to].bankrupt {
            return Err(CommandError::TradeInvalid);
        }
        let open_from_p = self
            .st
            .pending_trades
            .iter()
            .filter(|t| t.from == p)
            .count();
        if open_from_p >= MAX_OPEN_TRADES_PER_PLAYER {
            return Err(CommandError::TradeLimit);
        }
        if give_cash < 0 || receive_cash < 0 {
            return Err(CommandError::TradeInvalid);
        }
        let empty = give_cash == 0
            && receive_cash == 0
            && give_tiles.is_empty()
            && receive_tiles.is_empty();
        if empty {
            return Err(CommandError::TradeInvalid);
        }

        let offer = TradeOffer {
            id: self.st.trade_seq,
            from: p,
            to,
            give_cash,
            give_tiles: self.resolve_trade_tiles(give_tiles)?,
            receive_cash,
            receive_tiles: self.resolve_trade_tiles(receive_tiles)?,
        };
        self.validate_trade_assets(&offer)?;

        self.st.trade_seq += 1;
        self.ev.push(Event::TradeProposed {
            trade: offer.id,
            from: p,
            to,
        });
        self.st.pending_trades.push(offer);
        Ok(())
    }

    fn accept_trade(&mut self, p: usize, id: u32) -> Result<(), CommandError> {
        self.reject_during_auction()?;
        let idx = self.trade_index(id)?;
        let offer = self.st.pending_trades[idx].clone();
        if offer.to != p {
            return Err(CommandError::NotTradeParty);
        }
        // Ownership or cash may have shifted since the proposal. A stale
        // offer rejects here without mutating (ADR-0001); the recipient can
        // decline it to clear it out.
        self.validate_trade_assets(&offer)?;

        self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeAccepted {
            trade: id,
            from: offer.from,
            to: offer.to,
        });
        self.st.players[offer.from].cash += offer.receive_cash - offer.give_cash;
        self.st.players[offer.to].cash += offer.give_cash - offer.receive_cash;
        for &tile in &offer.give_tiles {
            self.st.tiles[tile].owner = Some(offer.to);
            self.st.tiles[tile].boosts = 0;
            self.ev.push(Event::PropertyTransferred {
                tile,
                from: offer.from,
                to: Some(offer.to),
            });
        }
        for &tile in &offer.receive_tiles {
            self.st.tiles[tile].owner = Some(offer.from);
            self.st.tiles[tile].boosts = 0;
            self.ev.push(Event::PropertyTransferred {
                tile,
                from: offer.to,
                to: Some(offer.from),
            });
        }
        Ok(())
    }

    fn decline_trade(&mut self, p: usize, id: u32) -> Result<(), CommandError> {
        let idx = self.trade_index(id)?;
        if self.st.pending_trades[idx].to != p {
            return Err(CommandError::NotTradeParty);
        }
        let offer = self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeDeclined {
            trade: id,
            from: offer.from,
            to: offer.to,
        });
        Ok(())
    }

    fn cancel_trade(&mut self, p: usize, id: u32) -> Result<(), CommandError> {
        let idx = self.trade_index(id)?;
        if self.st.pending_trades[idx].from != p {
            return Err(CommandError::NotTradeParty);
        }
        let offer = self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeCancelled {
            trade: id,
            from: offer.from,
            to: offer.to,
        });
        Ok(())
    }

    fn trade_index(&self, id: u32) -> Result<usize, CommandError> {
        self.st
            .pending_trades
            .iter()
            .position(|t| t.id == id)
            .ok_or(CommandError::TradeNotFound)
    }

    fn reject_during_auction(&self) -> Result<(), CommandError> {
        match self.st.turn {
            TurnPhase::BlindAuction { .. } => Err(CommandError::WrongPhase),
            _ => Ok(()),
        }
    }

    fn resolve_trade_tiles(&self, ids: &[String]) -> Result<Vec<usize>, CommandError> {
        let mut tiles = Vec::with_capacity(ids.len());
        for id in ids {
            let tile = self
                .content
                .tile_index(id)
                .ok_or_else(|| CommandError::UnknownTile { tile: id.clone() })?;
            if tiles.contains(&tile) {
                return Err(CommandError::TradeInvalid);
            }
            tiles.push(tile);
        }
        Ok(tiles)
    }

    /// Full asset check, run both at proposal and at acceptance time.
    fn validate_trade_assets(&self, offer: &TradeOffer) -> Result<(), CommandError> {
        for (&owner, tiles) in [
            (&offer.from, &offer.give_tiles),
            (&offer.to, &offer.receive_tiles),
        ] {
            for &tile in tiles {
                let prop = self
                    .content
                    .property(tile)
                    .ok_or(CommandError::NotAProperty)?;
                if self.st.tiles[tile].owner != Some(owner) {
                    return Err(CommandError::NotOwner);
                }
                if self
                    .content
                    .group_tiles(&prop.group)
                    .iter()
                    .any(|&t| self.st.tiles[t].houses > 0)
                {
                    return Err(CommandError::HousesInGroup);
                }
            }
        }
        if self.st.players[offer.from].cash < offer.give_cash
            || self.st.players[offer.to].cash < offer.receive_cash
        {
            return Err(CommandError::InsufficientFunds);
        }
        Ok(())
    }

    // -- Sealed-bid auction (ADR-0018) -------------------------------------------
    //
    // Every landing on an unowned property opens a 5s window (server-timed,
    // see crates/server/src/room.rs) in which every living seat submits
    // exactly one bid; `0` abstains. The discoverer (the landing player,
    // `GameState::current` - stable for the whole window, see the
    // turn-advance guard in `apply()`) is treated as having bid list price
    // if they stay silent/submit `0` and can afford it. Resolution is pure
    // and automatic the instant every living seat has bid - no close command.

    fn submit_blind_bid(&mut self, p: usize, amount: i64) -> Result<(), CommandError> {
        let TurnPhase::BlindAuction { tile, ref bids } = self.st.turn else {
            return Err(CommandError::WrongPhase);
        };
        if bids[p].is_some() {
            return Err(CommandError::AlreadyBid);
        }
        if !(0..=self.st.players[p].cash).contains(&amount) {
            return Err(CommandError::InsufficientFunds);
        }
        let floor = self
            .content
            .property(tile)
            .expect("BlindAuction always targets a property")
            .price;
        if p == self.st.current && amount != 0 && amount < floor {
            return Err(CommandError::BidBelowFloor);
        }
        let TurnPhase::BlindAuction { bids, .. } = &mut self.st.turn else {
            unreachable!()
        };
        bids[p] = Some(amount);
        self.ev.push(Event::BlindBidSubmitted { player: p });
        self.maybe_resolve_blind_auction();
        Ok(())
    }

    /// Resolves the open sealed-bid window once every living seat has bid.
    /// A no-op otherwise. Highest effective bid wins (the discoverer's
    /// silent/zero bid is substituted with the list price if they can
    /// afford it); ties favour the discoverer, then the lowest seat.
    fn maybe_resolve_blind_auction(&mut self) {
        let TurnPhase::BlindAuction { tile, ref bids } = self.st.turn else {
            return;
        };
        if !self.st.alive_players().all(|s| bids[s].is_some()) {
            return;
        }
        let discoverer = self.st.current;
        let floor = self
            .content
            .property(tile)
            .expect("BlindAuction always targets a property")
            .price;
        let raw: Vec<i64> = {
            let TurnPhase::BlindAuction { bids, .. } = &self.st.turn else {
                unreachable!()
            };
            (0..self.st.players.len())
                .map(|i| bids[i].unwrap_or(0))
                .collect()
        };
        let effective = |s: usize| -> i64 {
            if s == discoverer && raw[s] == 0 && self.st.players[discoverer].cash >= floor {
                floor
            } else {
                raw[s]
            }
        };
        let winner = self
            .st
            .alive_players()
            .filter(|&s| effective(s) > 0)
            .max_by_key(|&s| (effective(s), s == discoverer, std::cmp::Reverse(s)));
        match winner {
            Some(w) => {
                let win_amount = effective(w);
                let raw_settlement = if w == discoverer && win_amount > floor {
                    win_amount * 90 / 100
                } else {
                    win_amount
                };
                let settlement = self
                    .apply_market_multiplier(MarketEffect::AcquisitionMultiplier, raw_settlement);
                self.st.players[w].cash -= settlement;
                self.st.tiles[tile].owner = Some(w);
                self.ev.push(Event::BlindAuctionResolved {
                    tile,
                    discoverer,
                    winner: Some(w),
                    amount: settlement,
                    bids: raw,
                });
            }
            None => {
                self.ev.push(Event::BlindAuctionResolved {
                    tile,
                    discoverer,
                    winner: None,
                    amount: 0,
                    bids: raw,
                });
            }
        }
        self.st.turn = TurnPhase::AwaitEnd;
    }

    fn build(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        if !matches!(self.st.turn, TurnPhase::AwaitRoll | TurnPhase::AwaitEnd) {
            return Err(CommandError::WrongPhase);
        }
        let tile = self
            .content
            .tile_index(tile_id)
            .ok_or_else(|| CommandError::UnknownTile {
                tile: tile_id.to_string(),
            })?;
        let prop = self
            .content
            .property(tile)
            .ok_or(CommandError::NotAProperty)?;
        if self.st.tiles[tile].owner != Some(p) {
            return Err(CommandError::NotOwner);
        }
        if prop.rent_model != RentModel::Houses {
            return Err(CommandError::NotBuildable);
        }
        if !self.st.owns_full_group(self.content, p, &prop.group) {
            return Err(CommandError::GroupIncomplete);
        }
        if self
            .content
            .group_tiles(&prop.group)
            .iter()
            .any(|&t| self.st.tiles[t].mortgaged)
        {
            return Err(CommandError::MortgagedInGroup);
        }
        let cap = self.content.rules.max_houses_per_property.min(5);
        if self.st.tiles[tile].houses >= cap {
            return Err(CommandError::BuildLimit);
        }
        let group_min = self
            .content
            .group_tiles(&prop.group)
            .iter()
            .map(|&t| self.st.tiles[t].houses)
            .min()
            .unwrap_or(0);
        if self.st.tiles[tile].houses > group_min {
            return Err(CommandError::UnevenBuild);
        }
        if self.st.players[p].cash < prop.house_cost {
            return Err(CommandError::InsufficientFunds);
        }
        // Shared building pools (ADR-0019): the top level draws a
        // conglomerate and, in the same motion, releases the max-1
        // subsidiaries the tile held (the classic house-to-hotel
        // conversion); any other level draws a plain subsidiary.
        let becomes_top = self.st.tiles[tile].houses + 1 == cap;
        if becomes_top {
            self.st
                .take_conglomerate()
                .map_err(|()| CommandError::PoolExhausted)?;
        } else {
            self.st
                .take_subsidiary()
                .map_err(|()| CommandError::PoolExhausted)?;
        }
        self.st.players[p].cash -= prop.house_cost;
        self.st.tiles[tile].houses += 1;
        if becomes_top {
            self.st.return_subsidiaries((cap - 1) as u64);
        }
        self.ev.push(Event::HouseBuilt {
            player: p,
            tile,
            houses: self.st.tiles[tile].houses,
            cost: prop.house_cost,
        });
        Ok(())
    }

    fn sell_house(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        if prop.rent_model != RentModel::Houses {
            return Err(CommandError::NotBuildable);
        }
        if self.st.tiles[tile].houses == 0 {
            return Err(CommandError::NoHouses);
        }
        let group_max = self
            .content
            .group_tiles(&prop.group)
            .iter()
            .map(|&t| self.st.tiles[t].houses)
            .max()
            .unwrap_or(0);
        if self.st.tiles[tile].houses < group_max {
            return Err(CommandError::UnevenBuild);
        }
        // Shared building pools (ADR-0019): stepping down off the top level
        // returns the conglomerate but must re-issue max-1 subsidiaries -
        // rejected if the bank can't lend that many right now (mortgaging
        // remains the liquidity valve). Any other level just returns one
        // subsidiary, which can never fail.
        let cap = self.content.rules.max_houses_per_property.min(5);
        let steps_off_top = self.st.tiles[tile].houses == cap;
        if steps_off_top {
            let subsidiaries_needed = (cap - 1) as u64;
            if !self.st.subsidiaries_free(subsidiaries_needed) {
                return Err(CommandError::PoolExhausted);
            }
        }
        let refund = prop.house_cost / 2;
        self.st.tiles[tile].houses -= 1;
        self.st.players[p].cash += refund;
        if steps_off_top {
            self.st.return_conglomerate();
            self.st.consume_subsidiaries((cap - 1) as u64);
        } else {
            self.st.return_subsidiaries(1);
        }
        self.ev.push(Event::HouseSold {
            player: p,
            tile,
            houses: self.st.tiles[tile].houses,
            refund,
        });
        Ok(())
    }

    /// Seize a rival's unmortgaged property for a premium (ADR-0011). The
    /// former owner is compensated (min of price and what was paid); the
    /// bank keeps any premium above that. Takeover happens on the landing
    /// tile only (ADR-0022): after rent has resolved, at the end of the
    /// acting player's own turn, on the exact tile they are standing on.
    /// Improved tiles are seizable too (ADR-0022): their buildings
    /// liquidate at `sell_house` pricing, paid to the former owner on top
    /// of the usual compensation, and the stripped units return to the
    /// shared pools; the taker always receives a bare tile.
    fn expropriate(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        if !matches!(self.st.turn, TurnPhase::AwaitEnd) {
            return Err(CommandError::WrongPhase);
        }
        let pct = self.content.rules.expropriation;
        if pct <= 0 {
            return Err(CommandError::ExpropriationDisabled);
        }
        let tile = self
            .content
            .tile_index(tile_id)
            .ok_or_else(|| CommandError::UnknownTile {
                tile: tile_id.to_string(),
            })?;
        if self.st.players[p].position != tile {
            return Err(CommandError::NotOnTile);
        }
        let prop = self
            .content
            .property(tile)
            .ok_or(CommandError::NotAProperty)?;
        let ts = self.st.tiles[tile];
        // Must be a rival's property, mortgage-free (the takeover shield);
        // improved tiles are legal targets now (ADR-0022).
        let from = match ts.owner {
            Some(o) if o != p && !ts.mortgaged => o,
            _ => return Err(CommandError::NotExpropriable),
        };
        let cost = self
            .apply_market_multiplier(MarketEffect::AcquisitionMultiplier, prop.price * pct / 100);
        if self.st.players[p].cash < cost {
            return Err(CommandError::InsufficientFunds);
        }
        let compensation = prop.price.min(cost);
        let cap = self.content.rules.max_houses_per_property.min(5);
        let liquidated = ts.houses;
        let liquidation_refund = (prop.house_cost / 2) * liquidated as i64;
        self.st.players[p].cash -= cost;
        self.st.players[from].cash += compensation + liquidation_refund;
        self.st.tiles[tile].owner = Some(p);
        self.st.tiles[tile].houses = 0;
        self.st.tiles[tile].boosts = 0;
        self.st.release_tile_pools(liquidated, cap);
        self.ev.push(Event::Expropriated {
            player: p,
            from,
            tile,
            cost,
            liquidated,
            liquidation_refund,
        });
        Ok(())
    }

    /// Raise an owned tile's rent one step for a fee (ADR-0012), up to
    /// `MAX_RENT_BOOSTS`. Mortgaged tiles cannot be boosted.
    fn boost_rent(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        let pct = self.content.rules.rent_boost;
        if pct <= 0 {
            return Err(CommandError::RentBoostDisabled);
        }
        if self.st.tiles[tile].mortgaged {
            return Err(CommandError::AlreadyMortgaged);
        }
        if self.st.tiles[tile].boosts >= MAX_RENT_BOOSTS {
            return Err(CommandError::BoostLimit);
        }
        let cost = prop.price * pct / 100;
        if self.st.players[p].cash < cost {
            return Err(CommandError::InsufficientFunds);
        }
        self.st.players[p].cash -= cost;
        self.st.tiles[tile].boosts += 1;
        self.ev.push(Event::RentBoosted {
            player: p,
            tile,
            boosts: self.st.tiles[tile].boosts,
            cost,
        });
        Ok(())
    }

    fn mortgage(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        if self.st.tiles[tile].mortgaged {
            return Err(CommandError::AlreadyMortgaged);
        }
        // Classic rule: the whole group must be building-free first.
        if self
            .content
            .group_tiles(&prop.group)
            .iter()
            .any(|&t| self.st.tiles[t].houses > 0)
        {
            return Err(CommandError::HousesInGroup);
        }
        let value = prop.price / 2;
        self.st.tiles[tile].mortgaged = true;
        self.st.players[p].cash += value;
        self.ev.push(Event::PropertyMortgaged {
            player: p,
            tile,
            value,
        });
        Ok(())
    }

    fn unmortgage(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        if !self.st.tiles[tile].mortgaged {
            return Err(CommandError::NotMortgaged);
        }
        let principal = prop.price / 2;
        let cost = principal + principal / 10; // 10% interest, floored
        // Voluntary payment never forces liquidation: reject if unaffordable.
        if self.st.players[p].cash < cost {
            return Err(CommandError::InsufficientFunds);
        }
        self.st.players[p].cash -= cost;
        self.st.tiles[tile].mortgaged = false;
        self.ev.push(Event::PropertyUnmortgaged {
            player: p,
            tile,
            cost,
        });
        Ok(())
    }

    /// Shared validation for tile-targeted asset commands (build phases).
    /// Returns the def borrowed from `content` (not `self`) so callers can
    /// keep it across later state mutations.
    fn owned_property(
        &self,
        p: usize,
        tile_id: &str,
    ) -> Result<(usize, &'e PropertyDef), CommandError> {
        if !matches!(self.st.turn, TurnPhase::AwaitRoll | TurnPhase::AwaitEnd) {
            return Err(CommandError::WrongPhase);
        }
        let content: &'e GameContent = self.content;
        let tile = content
            .tile_index(tile_id)
            .ok_or_else(|| CommandError::UnknownTile {
                tile: tile_id.to_string(),
            })?;
        let prop = content.property(tile).ok_or(CommandError::NotAProperty)?;
        if self.st.tiles[tile].owner != Some(p) {
            return Err(CommandError::NotOwner);
        }
        Ok((tile, prop))
    }

    fn pay_jail_fine(&mut self, p: usize) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitRoll {
            return Err(CommandError::WrongPhase);
        }
        if self.st.players[p].jail_turns.is_none() {
            return Err(CommandError::NotInJail);
        }
        let fine = self.content.rules.jail_fine;
        // Voluntary payment never forces liquidation: reject if unaffordable.
        if self.st.players[p].cash < fine {
            return Err(CommandError::InsufficientFunds);
        }
        self.st.players[p].cash -= fine;
        self.st.players[p].jail_turns = None;
        self.ev.push(Event::JailFinePaid {
            player: p,
            amount: fine,
        });
        self.ev.push(Event::LeftJail { player: p });
        Ok(())
    }

    fn use_jail_card(&mut self, p: usize) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitRoll {
            return Err(CommandError::WrongPhase);
        }
        if self.st.players[p].jail_turns.is_none() {
            return Err(CommandError::NotInJail);
        }
        if self.st.players[p].jail_cards == 0 {
            return Err(CommandError::NoJailCard);
        }
        self.st.players[p].jail_cards -= 1;
        self.st.players[p].jail_turns = None;
        self.ev.push(Event::JailCardUsed { player: p });
        self.ev.push(Event::LeftJail { player: p });
        Ok(())
    }

    fn end_turn(&mut self, p: usize) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitEnd {
            return Err(CommandError::WrongPhase);
        }
        let extra_roll =
            self.st.players[p].doubles_streak > 0 && self.st.players[p].jail_turns.is_none();
        if extra_roll {
            self.st.turn = TurnPhase::AwaitRoll;
        } else {
            self.advance_turn();
        }
        Ok(())
    }

    fn resign(&mut self, p: usize) -> Result<(), CommandError> {
        self.ev.push(Event::PlayerResigned { player: p });
        self.bankrupt(p, None);
        // Bankruptcy already excluded `p` from `alive_players()`, so this
        // may complete a sealed-bid window still waiting on `p` - including
        // the discoverer resigning while other seats haven't bid yet.
        if matches!(self.st.phase, GamePhase::Active)
            && matches!(self.st.turn, TurnPhase::BlindAuction { .. })
        {
            self.maybe_resolve_blind_auction();
        }
        Ok(())
    }

    // -- Movement -------------------------------------------------------------

    fn move_forward(&mut self, p: usize, steps: usize) {
        let len = self.content.board.len();
        let from = self.st.players[p].position;
        let raw = from + steps;
        let passed_go = raw >= len;
        let to = raw % len;
        self.st.players[p].position = to;
        self.ev.push(Event::Moved {
            player: p,
            from,
            to,
            passed_go,
        });
        if passed_go {
            self.pay_salary(p);
        }
    }

    /// Direct placement (cards). Salary is granted only for forward wraps
    /// when the card says so; backward moves never pay.
    fn teleport(&mut self, p: usize, to: usize, collect_go: bool) {
        let from = self.st.players[p].position;
        let passed_go = collect_go && to <= from && to != from;
        let passed_go = passed_go || (collect_go && to == 0 && from != 0);
        self.st.players[p].position = to;
        self.ev.push(Event::Moved {
            player: p,
            from,
            to,
            passed_go,
        });
        if passed_go {
            self.pay_salary(p);
        }
    }

    fn pay_salary(&mut self, p: usize) {
        let amount = self.content.rules.go_salary;
        self.st.players[p].cash += amount;
        self.ev.push(Event::SalaryPaid { player: p, amount });
    }

    fn go_to_jail(&mut self, p: usize) {
        self.st.players[p].position = self.content.jail_position();
        self.st.players[p].jail_turns = Some(0);
        self.st.players[p].doubles_streak = 0;
        self.ev.push(Event::WentToJail { player: p });
    }

    /// Applies a tile's rent-boost level to a base rent (ADR-0012):
    /// `+RENT_BOOST_STEP_PCT%` per boost.
    fn boosted_rent(base: i64, boosts: u8) -> i64 {
        base * (100 + RENT_BOOST_STEP_PCT * boosts as i64) / 100
    }

    /// Applies the active market event's magnitude to `base` if it matches
    /// `effect` (ADR-0021); a no-op otherwise, including while nothing is
    /// active. Shared by rent (`resolve_landing`) and takeover cost
    /// (`expropriate`).
    fn apply_market_multiplier(&self, effect: MarketEffect, base: i64) -> i64 {
        match &self.st.forecast.active {
            Some(active) if active.effect == effect => {
                (base * (100 + active.magnitude_pct) / 100).max(0)
            }
            _ => base,
        }
    }

    // -- Landing resolution -----------------------------------------------------

    fn resolve_landing(&mut self, p: usize, dice_total: u8, depth: u8) {
        if depth > MAX_CARD_CHAIN_DEPTH {
            self.st.turn = TurnPhase::AwaitEnd;
            return;
        }
        let tile = self.st.players[p].position;
        match &self.content.board[tile].kind {
            TileKind::Go | TileKind::Jail | TileKind::FreeParking => {
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::GoToJail => {
                self.go_to_jail(p);
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::Tax { amount } => {
                let amount = *amount;
                self.ev.push(Event::TaxPaid {
                    player: p,
                    tile,
                    amount,
                });
                self.charge(p, None, amount);
                self.st.turn = TurnPhase::AwaitEnd;
            }
            TileKind::Property(prop) => match self.st.tiles[tile].owner {
                None => {
                    self.ev.push(Event::BlindAuctionOpened {
                        tile,
                        discoverer: p,
                        floor: prop.price,
                    });
                    self.st.turn = TurnPhase::BlindAuction {
                        tile,
                        bids: vec![None; self.st.players.len()],
                    };
                }
                Some(owner) if owner == p => {
                    self.st.turn = TurnPhase::AwaitEnd;
                }
                Some(_) if self.st.tiles[tile].mortgaged => {
                    self.st.turn = TurnPhase::AwaitEnd;
                }
                Some(owner) => {
                    let base = self
                        .strat
                        .rent
                        .rent(self.content, &self.st, tile, dice_total);
                    let rent = Self::boosted_rent(base, self.st.tiles[tile].boosts);
                    let rent = self.apply_market_multiplier(MarketEffect::RentMultiplier, rent);
                    self.ev.push(Event::RentPaid {
                        from: p,
                        to: owner,
                        tile,
                        amount: rent,
                    });
                    self.charge(p, Some(owner), rent);
                    self.st.turn = TurnPhase::AwaitEnd;
                }
            },
            TileKind::Chance => self.draw_card(p, DeckKind::Chance, dice_total, depth),
            TileKind::Community => self.draw_card(p, DeckKind::Community, dice_total, depth),
        }
    }

    fn draw_card(&mut self, p: usize, deck: DeckKind, dice_total: u8, depth: u8) {
        let idx = match deck {
            DeckKind::Chance => self.st.chance_deck.draw(),
            DeckKind::Community => self.st.community_deck.draw(),
        };
        let Some(idx) = idx else {
            // Validated content never hits this; mod-broken decks degrade to a no-op.
            self.st.turn = TurnPhase::AwaitEnd;
            return;
        };
        let card = match deck {
            DeckKind::Chance => self.content.chance[idx].clone(),
            DeckKind::Community => self.content.community[idx].clone(),
        };
        self.ev.push(Event::CardDrawn {
            player: p,
            deck,
            card: card.id.clone(),
            text: card.text.clone(),
        });
        self.apply_card_effect(p, &card.id, &card.effect, dice_total, depth);
    }

    fn apply_card_effect(
        &mut self,
        p: usize,
        card_id: &str,
        effect: &CardEffect,
        dice_total: u8,
        depth: u8,
    ) {
        match effect {
            CardEffect::Money { amount } => {
                if *amount >= 0 {
                    self.st.players[p].cash += amount;
                    self.ev.push(Event::CashAdjusted {
                        player: p,
                        delta: *amount,
                        reason: card_id.to_string(),
                    });
                } else {
                    self.ev.push(Event::CashAdjusted {
                        player: p,
                        delta: *amount,
                        reason: card_id.to_string(),
                    });
                    self.charge(p, None, -amount);
                }
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::MoveTo { tile, collect_go } => {
                let to = self
                    .content
                    .tile_index(tile)
                    .expect("validated content: card targets exist");
                self.teleport(p, to, *collect_go);
                self.resolve_landing(p, dice_total, depth + 1);
            }
            CardEffect::MoveBy { steps } => {
                if *steps >= 0 {
                    self.move_forward(p, *steps as usize);
                } else {
                    let len = self.content.board.len() as i64;
                    let from = self.st.players[p].position as i64;
                    let to = (from + *steps as i64).rem_euclid(len) as usize;
                    self.teleport(p, to, false);
                }
                self.resolve_landing(p, dice_total, depth + 1);
            }
            CardEffect::GoToJail => {
                self.go_to_jail(p);
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::GetOutOfJail => {
                self.st.players[p].jail_cards += 1;
                self.ev.push(Event::JailCardReceived { player: p });
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::CollectFromEach { amount } => {
                let others: Vec<usize> = self.st.alive_players().filter(|&o| o != p).collect();
                for o in others {
                    self.ev.push(Event::CashAdjusted {
                        player: o,
                        delta: -amount,
                        reason: card_id.to_string(),
                    });
                    self.charge(o, Some(p), *amount);
                    if matches!(self.st.phase, GamePhase::Finished { .. }) {
                        return;
                    }
                }
                self.st.turn = TurnPhase::AwaitEnd;
            }
            CardEffect::PayEach { amount } => {
                let others: Vec<usize> = self.st.alive_players().filter(|&o| o != p).collect();
                for o in others {
                    self.ev.push(Event::CashAdjusted {
                        player: p,
                        delta: -amount,
                        reason: card_id.to_string(),
                    });
                    self.charge(p, Some(o), *amount);
                    if self.st.players[p].bankrupt
                        || matches!(self.st.phase, GamePhase::Finished { .. })
                    {
                        return;
                    }
                }
                self.st.turn = TurnPhase::AwaitEnd;
            }
        }
    }

    // -- Money and bankruptcy -----------------------------------------------------

    /// Moves `amount` from `debtor` to `creditor` (`None` = bank). Triggers
    /// liquidation, then bankruptcy, when cash cannot stay above the
    /// configured threshold. Semantic events (rent, tax, ...) are emitted by
    /// callers; this only emits distress events.
    fn charge(&mut self, debtor: usize, creditor: Option<usize>, amount: i64) {
        if amount <= 0 {
            return;
        }
        let threshold = self.content.rules.bankruptcy_threshold;
        let needed = amount + threshold;
        if self.st.players[debtor].cash < needed {
            self.strat.bankruptcy.liquidate(
                self.content,
                &mut self.st,
                debtor,
                needed,
                &mut self.ev,
            );
        }
        if self.st.players[debtor].cash >= needed {
            self.st.players[debtor].cash -= amount;
            if let Some(c) = creditor {
                self.st.players[c].cash += amount;
            }
            return;
        }
        // Partial settlement: the creditor receives whatever cash remains.
        let remaining = self.st.players[debtor].cash.max(0);
        self.st.players[debtor].cash -= remaining;
        if let Some(c) = creditor {
            self.st.players[c].cash += remaining;
        }
        self.bankrupt(debtor, creditor);
    }

    fn bankrupt(&mut self, p: usize, creditor: Option<usize>) {
        self.st.pending_trades.retain(|t| t.from != p && t.to != p);
        let cap = self.content.rules.max_houses_per_property.min(5);
        for tile in 0..self.st.tiles.len() {
            if self.st.tiles[tile].owner == Some(p) {
                // Bank refurbishes (no compensation), but the shared pools
                // still get their units back (ADR-0019) - a pure release.
                self.st.release_tile_pools(self.st.tiles[tile].houses, cap);
                self.st.tiles[tile].owner = creditor;
                self.st.tiles[tile].houses = 0;
                self.st.tiles[tile].boosts = 0;
                if creditor.is_none() {
                    // Returned to the bank: sold clean next time.
                    self.st.tiles[tile].mortgaged = false;
                }
                self.ev.push(Event::PropertyTransferred {
                    tile,
                    from: p,
                    to: creditor,
                });
            }
        }
        let player = &mut self.st.players[p];
        player.bankrupt = true;
        player.jail_turns = None;
        player.doubles_streak = 0;
        player.jail_cards = 0;
        self.ev.push(Event::PlayerBankrupt {
            player: p,
            creditor,
        });
        self.check_win();
    }

    // -- Turn flow ----------------------------------------------------------------

    fn advance_turn(&mut self) {
        if !matches!(self.st.phase, GamePhase::Active) {
            return;
        }
        self.st.players[self.st.current].doubles_streak = 0;
        let n = self.st.players.len();
        let mut next = self.st.current;
        for _ in 0..n {
            next = (next + 1) % n;
            if !self.st.players[next].bankrupt {
                break;
            }
        }
        self.st.current = next;
        self.st.turn = TurnPhase::AwaitRoll;
        self.st.turn_count += 1;
        self.ev.push(Event::TurnStarted { player: next });
        self.tick_forecast();
    }

    // -- Market forecast (ADR-0021) ------------------------------------------

    /// Turn-transition tick for the public forecast: expires the active
    /// effect if its window closed, activates the next scheduled event if
    /// it's due (a `WealthTax` resolves instantly here and never becomes
    /// "active" - nothing to expire), then refills the queue back to 3.
    /// Naturally a no-op when the content ships no market events: the
    /// queue can never hold anything to activate, and `draw_next` itself
    /// no-ops on an empty pool - no need for an explicit early return, and
    /// none here on purpose so an `active` effect (however it got there)
    /// always still expires on schedule.
    fn tick_forecast(&mut self) {
        if let Some(active) = &self.st.forecast.active
            && self.st.turn_count >= active.ends_at_turn
        {
            let event_id = active.event_id.clone();
            self.st.forecast.active = None;
            self.ev.push(Event::MarketEventExpired { event_id });
        }
        let due = self
            .st
            .forecast
            .queue
            .first()
            .is_some_and(|next| self.st.turn_count >= next.starts_at_turn);
        if self.st.forecast.active.is_none() && due {
            let scheduled = self.st.forecast.queue.remove(0);
            if let Some(def) = self.content.market_event(&scheduled.event_id) {
                let effect = def.effect;
                let magnitude_pct = def.magnitude_pct;
                self.ev.push(Event::MarketEventActivated {
                    event_id: scheduled.event_id.clone(),
                    effect,
                    magnitude_pct,
                    duration_turns: scheduled.duration,
                });
                if effect == MarketEffect::WealthTax {
                    self.apply_wealth_tax(magnitude_pct, &scheduled.event_id);
                } else {
                    self.st.forecast.active = Some(ActiveMarketEvent {
                        event_id: scheduled.event_id,
                        effect,
                        magnitude_pct,
                        ends_at_turn: self.st.turn_count + scheduled.duration,
                    });
                }
            }
            self.st
                .forecast
                .draw_next(self.content, &mut self.st.rng, self.st.turn_count);
        }
    }

    /// One-shot wealth tax (ADR-0021): every alive player pays `net_worth *
    /// pct / 100` through the normal charge/bankruptcy machinery, mirroring
    /// `CardEffect::CollectFromEach`/`PayEach`.
    fn apply_wealth_tax(&mut self, pct: i64, event_id: &str) {
        for p in self.st.alive_players().collect::<Vec<_>>() {
            let amount = (self.st.net_worth(self.content, p) * pct / 100).max(0);
            self.ev.push(Event::CashAdjusted {
                player: p,
                delta: -amount,
                reason: event_id.to_string(),
            });
            self.charge(p, None, amount);
            if matches!(self.st.phase, GamePhase::Finished { .. }) {
                return;
            }
        }
    }

    fn check_win(&mut self) {
        let winner = {
            let mut alive = self.st.alive_players();
            match (alive.next(), alive.next()) {
                (Some(winner), None) => Some(winner),
                _ => None,
            }
        };
        if let Some(winner) = winner {
            self.st.phase = GamePhase::Finished { winner };
            self.ev.push(Event::GameEnded { winner });
        }
    }

    /// Instant win by controlling `rules.win_full_groups` complete colour
    /// groups (ADR-0013). Lowest seat wins if two qualify at once (a trade).
    fn check_group_win(&mut self) {
        if !matches!(self.st.phase, GamePhase::Active) {
            return;
        }
        let need = self.content.rules.win_full_groups;
        if need <= 0 {
            return;
        }
        for p in self.st.alive_players().collect::<Vec<_>>() {
            let owned = self.st.full_groups_owned(self.content, p);
            if owned as i64 >= need {
                self.st.phase = GamePhase::Finished { winner: p };
                self.ev.push(Event::WonByGroups {
                    winner: p,
                    groups: owned.min(u8::MAX as usize) as u8,
                });
                return;
            }
        }
    }
}
