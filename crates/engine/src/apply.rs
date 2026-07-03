//! The command pipeline: validate -> apply -> emit events.
//!
//! `apply` works on an owned clone of the caller's state; rejected commands
//! therefore never leak partial mutations. All movement, payment, and
//! bankruptcy logic lives here; content lookups go through the registries
//! and rule fragments go through the injected strategies.

use crate::command::{CommandKind, PlayerCommand};
use crate::content::{CardEffect, GameContent, PropertyDef, RentModel, TileKind};
use crate::error::CommandError;
use crate::event::{DeckKind, Event};
use crate::state::{GamePhase, GameState, TradeOffer, TurnPhase};
use crate::{Engine, Strategies};

/// Card chains ("advance to X" landing on another card tile) are bounded to
/// keep resolution finite regardless of mod content.
const MAX_CARD_CHAIN_DEPTH: u8 = 4;
/// Anti-spam cap on standing offers per proposer.
const MAX_OPEN_TRADES_PER_PLAYER: usize = 4;

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
    let expected_actor = match state.turn {
        TurnPhase::Auction { turn, .. } => turn,
        _ => state.current,
    };
    let any_turn = matches!(
        cmd.kind,
        CommandKind::Resign
            | CommandKind::ProposeTrade { .. }
            | CommandKind::AcceptTrade { .. }
            | CommandKind::DeclineTrade { .. }
            | CommandKind::CancelTrade { .. }
    );
    if !any_turn && player != expected_actor {
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
        CommandKind::Buy => exec.buy(player)?,
        CommandKind::Decline => exec.decline(player)?,
        CommandKind::Bid { amount } => exec.bid(player, *amount)?,
        CommandKind::Pass => exec.pass(player)?,
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
        CommandKind::Mortgage { tile } => exec.mortgage(player, tile)?,
        CommandKind::Unmortgage { tile } => exec.unmortgage(player, tile)?,
        CommandKind::PayJailFine => exec.pay_jail_fine(player)?,
        CommandKind::UseJailCard => exec.use_jail_card(player)?,
        CommandKind::EndTurn => exec.end_turn(player)?,
        CommandKind::Resign => exec.resign(player)?,
    }

    // A player can go bankrupt during their own turn (jail fine, card debt).
    // The turn must then move on without requiring further input from them.
    if matches!(exec.st.phase, GamePhase::Active) && exec.st.players[exec.st.current].bankrupt {
        exec.advance_turn();
    }

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

    fn buy(&mut self, p: usize) -> Result<(), CommandError> {
        let tile = match self.st.turn {
            TurnPhase::AwaitBuy { tile } => tile,
            _ => return Err(CommandError::WrongPhase),
        };
        let price = self
            .content
            .property(tile)
            .expect("AwaitBuy always targets a property")
            .price;
        if self.st.players[p].cash < price {
            return Err(CommandError::InsufficientFunds);
        }
        self.st.players[p].cash -= price;
        self.st.tiles[tile].owner = Some(p);
        self.ev.push(Event::PropertyPurchased {
            player: p,
            tile,
            price,
        });
        self.st.turn = TurnPhase::AwaitEnd;
        Ok(())
    }

    fn decline(&mut self, p: usize) -> Result<(), CommandError> {
        let tile = match self.st.turn {
            TurnPhase::AwaitBuy { tile } => tile,
            _ => return Err(CommandError::WrongPhase),
        };
        self.ev.push(Event::PurchaseDeclined { player: p, tile });
        if self.content.rules.auction_on_decline {
            self.start_auction(tile);
        } else {
            self.st.turn = TurnPhase::AwaitEnd;
        }
        Ok(())
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
            self.ev.push(Event::PropertyTransferred {
                tile,
                from: offer.from,
                to: Some(offer.to),
            });
        }
        for &tile in &offer.receive_tiles {
            self.st.tiles[tile].owner = Some(offer.from);
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
        self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeDeclined { trade: id });
        Ok(())
    }

    fn cancel_trade(&mut self, p: usize, id: u32) -> Result<(), CommandError> {
        let idx = self.trade_index(id)?;
        if self.st.pending_trades[idx].from != p {
            return Err(CommandError::NotTradeParty);
        }
        self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeCancelled { trade: id });
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
            TurnPhase::Auction { .. } => Err(CommandError::WrongPhase),
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

    // -- Auction ----------------------------------------------------------------

    fn start_auction(&mut self, tile: usize) {
        let mut active = 0u8;
        for (i, player) in self.st.players.iter().enumerate() {
            if !player.bankrupt {
                active |= 1 << i;
            }
        }
        self.ev.push(Event::AuctionStarted { tile });
        // Bidding starts left of the decliner; the decliner speaks last.
        self.st.turn = TurnPhase::Auction {
            tile,
            high_bid: 0,
            high_bidder: None,
            turn: self.st.current,
            active,
        };
        self.advance_auction();
    }

    fn bid(&mut self, p: usize, amount: i64) -> Result<(), CommandError> {
        let TurnPhase::Auction {
            tile,
            high_bid,
            turn,
            active,
            ..
        } = self.st.turn
        else {
            return Err(CommandError::WrongPhase);
        };
        if amount <= high_bid || amount < 1 {
            return Err(CommandError::BidTooLow);
        }
        // Cash cannot change during an auction, so validating here guarantees
        // the winner can pay at settlement.
        if self.st.players[p].cash < amount {
            return Err(CommandError::InsufficientFunds);
        }
        self.ev.push(Event::BidPlaced {
            player: p,
            tile,
            amount,
        });
        self.st.turn = TurnPhase::Auction {
            tile,
            high_bid: amount,
            high_bidder: Some(p),
            turn,
            active,
        };
        self.advance_auction();
        Ok(())
    }

    fn pass(&mut self, p: usize) -> Result<(), CommandError> {
        let TurnPhase::Auction {
            tile,
            high_bid,
            high_bidder,
            turn,
            active,
        } = self.st.turn
        else {
            return Err(CommandError::WrongPhase);
        };
        self.ev.push(Event::AuctionPassed { player: p, tile });
        self.st.turn = TurnPhase::Auction {
            tile,
            high_bid,
            high_bidder,
            turn,
            active: active & !(1 << p),
        };
        self.advance_auction();
        Ok(())
    }

    /// Moves the auction to the next seat that may speak (active and not the
    /// current high bidder). When nobody is left to speak, settles.
    fn advance_auction(&mut self) {
        let TurnPhase::Auction {
            tile,
            high_bid,
            high_bidder,
            turn,
            active,
        } = self.st.turn
        else {
            return;
        };
        let n = self.st.players.len();
        let mut i = turn;
        for _ in 0..n {
            i = (i + 1) % n;
            if active & (1 << i) != 0 && Some(i) != high_bidder {
                self.st.turn = TurnPhase::Auction {
                    tile,
                    high_bid,
                    high_bidder,
                    turn: i,
                    active,
                };
                return;
            }
        }
        match high_bidder {
            Some(winner) => {
                self.st.players[winner].cash -= high_bid;
                self.st.tiles[tile].owner = Some(winner);
                self.ev.push(Event::AuctionEnded {
                    tile,
                    winner: Some(winner),
                    amount: high_bid,
                });
            }
            None => self.ev.push(Event::AuctionEnded {
                tile,
                winner: None,
                amount: 0,
            }),
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
        self.st.players[p].cash -= prop.house_cost;
        self.st.tiles[tile].houses += 1;
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
        let refund = prop.house_cost / 2;
        self.st.tiles[tile].houses -= 1;
        self.st.players[p].cash += refund;
        self.ev.push(Event::HouseSold {
            player: p,
            tile,
            houses: self.st.tiles[tile].houses,
            refund,
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
        if let TurnPhase::Auction {
            tile,
            high_bid,
            high_bidder,
            turn,
            active,
        } = self.st.turn
        {
            // A resigning high bidder forfeits: bidding reopens from zero
            // (rare edge; slight discount for the remaining bidders).
            let (high_bid, high_bidder) = if high_bidder == Some(p) {
                (0, None)
            } else {
                (high_bid, high_bidder)
            };
            self.st.turn = TurnPhase::Auction {
                tile,
                high_bid,
                high_bidder,
                turn,
                active: active & !(1 << p),
            };
            if turn == p {
                self.advance_auction();
            }
        }
        self.bankrupt(p, None);
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
                    self.ev.push(Event::PurchaseOffered {
                        player: p,
                        tile,
                        price: prop.price,
                    });
                    self.st.turn = TurnPhase::AwaitBuy { tile };
                }
                Some(owner) if owner == p => {
                    self.st.turn = TurnPhase::AwaitEnd;
                }
                Some(_) if self.st.tiles[tile].mortgaged => {
                    self.st.turn = TurnPhase::AwaitEnd;
                }
                Some(owner) => {
                    let rent = self
                        .strat
                        .rent
                        .rent(self.content, &self.st, tile, dice_total);
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
        for tile in 0..self.st.tiles.len() {
            if self.st.tiles[tile].owner == Some(p) {
                self.st.tiles[tile].owner = creditor;
                self.st.tiles[tile].houses = 0;
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
}
