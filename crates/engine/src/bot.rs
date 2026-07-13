//! Autopilot heuristics for playtesting and server-side bot seats: simple,
//! safe decisions over the public `ClientView`.
//!
//! Pure and synchronous like
//! the rest of the engine (no I/O, no rand, no clock) - it only reads a view
//! and the content and returns a command, so the server stays authoritative
//! and a buggy bot can at worst get its commands rejected. See ADR-0014 for
//! why this lives in the engine rather than a client.
//!
//! Unpredictability comes from the caller: `decide` takes a `noise` word
//! (any `u64` - the session layer/CLI pass a random one) that seeds a local
//! `SplitMix64` stream for the bid jitter. Same inputs, same output - the
//! engine itself still never draws ambient randomness.

use crate::rng;
use crate::tuning::{MORTGAGE_INTEREST_PCT, MORTGAGE_VALUE_PCT};
use crate::{ClientView, CommandKind, GameContent, GamePhase, RentModel, TileKind, TurnPhase};

/// Cash the bot refuses to dip under when buying or bidding.
const RESERVE: i64 = 100;
/// Extra comfort required before sinking money into houses.
const BUILD_RESERVE: i64 = 300;
/// Extra comfort required before redeeming mortgages.
const REDEEM_RESERVE: i64 = 500;
use crate::tuning::MAX_RENT_BOOSTS;
/// Minimum trade edge before the bot accepts an offer.
const TRADE_MARGIN: i64 = 25;
/// Sealed-bid jitter bounds, as percent of list price (2026-07: bots got
/// unpredictable on purpose - fixed formulas were trivial to outbid).
const BID_JITTER_MIN_PCT: i64 = 50;
const BID_JITTER_MAX_PCT: i64 = 200;
/// Below this fraction of list price the bot abstains instead of
/// lowballing a bid that could never win against the discoverer floor.
const MIN_BID_PCT: i64 = 50;
/// Landing-score weights for movement-card choice (heuristic, not rules).
const SCORE_GO_TO_JAIL: i64 = -1000;
const SCORE_BUYABLE_UNOWNED: i64 = 20;
const SCORE_OWN_TILE: i64 = 5;
const SCORE_NEUTRAL: i64 = 1;
/// Rival-tile penalty, percent of its price (steer away from the pricey).
const RIVAL_TILE_PENALTY_PCT: i64 = 5;
/// Strategic premium on group-completing/completed tiles, percent of price.
const GROUP_PREMIUM_PCT: i64 = 50;
/// Baseline priority weight per tile, percent of price (tie-breaker).
const PRIORITY_PRICE_PCT: i64 = 10;

/// What the bot wants to do right now, if anything. Pure given its inputs
/// (`noise` included) and idempotent: called after every server update,
/// returns `None` whenever it is not this seat's move.
#[must_use]
pub fn decide(
    content: &GameContent,
    view: &ClientView,
    me: usize,
    noise: u64,
) -> Option<CommandKind> {
    if matches!(view.phase, GamePhase::Finished { .. }) || view.players[me].bankrupt {
        return None;
    }

    let bot = Bot {
        content,
        view,
        me,
        noise,
    };
    if let Some(command) = bot.trade_response() {
        return Some(command);
    }

    match &view.turn {
        TurnPhase::BlindAuction { tile, bids } => bot.blind_bid_action(*tile, bids),
        TurnPhase::BribeVote {
            briber,
            amount,
            votes,
        } => bot.bribe_vote_action(*briber, *amount, votes),
        _ if view.current != me => None,
        TurnPhase::AwaitMove => {
            if view.players[me].in_jail {
                Some(bot.jail_action())
            } else if let Some(route) = &view.players[me].jail_route
                && let Some(&value) = route.first()
            {
                Some(CommandKind::PlayMovementCard { value })
            } else {
                bot.asset_action().or_else(|| bot.choose_movement_card())
            }
        }
        TurnPhase::AwaitEnd => bot.asset_action().or(Some(CommandKind::EndTurn)),
    }
}

/// Just the movement-card choice for `seat` (the tile-scoring heuristic
/// `decide` uses in `AwaitMove`), or `None` when it is not a plain move
/// (jailed, mid-route, empty hand, or not this seat's turn).
///
/// The session
/// layer uses this to auto-play a timed-out seat's *movement* with bot
/// smarts instead of the dumb lowest-card canonical action (2026-07) -
/// deliberately scoped to movement, so an AFK auto-play never spends the
/// player's money (building, bribing, boosting) behind their back.
#[must_use]
pub fn movement_card(content: &GameContent, view: &ClientView, seat: usize) -> Option<u8> {
    if view.current != seat || !matches!(view.turn, TurnPhase::AwaitMove) {
        return None;
    }
    let player = &view.players[seat];
    if player.in_jail || player.jail_route.is_some() {
        return None;
    }
    let bot = Bot {
        content,
        view,
        me: seat,
        noise: 0,
    };
    match bot.choose_movement_card() {
        Some(CommandKind::PlayMovementCard { value }) => Some(value),
        _ => None,
    }
}

struct Bot<'a> {
    content: &'a GameContent,
    view: &'a ClientView,
    me: usize,
    /// Caller-provided randomness for the bid jitter; see the module doc.
    noise: u64,
}

impl Bot<'_> {
    fn cash(&self) -> i64 {
        self.view.players[self.me].cash
    }

    fn cash_after(&self, cost: i64) -> i64 {
        self.cash() - cost
    }

    fn trade_response(&self) -> Option<CommandKind> {
        let offer = self.view.pending_trades.iter().find(|t| t.to == self.me)?;
        if !self.can_fulfill_trade_payment(offer.receive_cash, &offer.receive_tiles) {
            return Some(CommandKind::DeclineTrade { trade: offer.id });
        }

        let incoming = offer.give_cash + self.tiles_value(&offer.give_tiles);
        let outgoing = offer.receive_cash + self.tiles_value(&offer.receive_tiles);
        if incoming >= outgoing + TRADE_MARGIN {
            Some(CommandKind::AcceptTrade { trade: offer.id })
        } else {
            Some(CommandKind::DeclineTrade { trade: offer.id })
        }
    }

    /// Sealed-bid auction (ADR-0018): each seat submits exactly once. Bids
    /// a jittered 50-200% of list price (2026-07 playtest decision -
    /// fixed-formula bots were perfectly predictable to bid against),
    /// clamped to what the seat can afford. The discoverer never bids
    /// below its own implicit floor (an explicit sub-floor bid would be
    /// rejected); a seat that can't cover half the price abstains.
    fn blind_bid_action(&self, tile: usize, bids: &[Option<i64>]) -> Option<CommandKind> {
        if bids[self.me].is_some() {
            return None;
        }
        let price = self.content.property(tile)?.price;
        let mut noise = self.noise;
        let pct = BID_JITTER_MIN_PCT
            + rng::below(
                &mut noise,
                (BID_JITTER_MAX_PCT - BID_JITTER_MIN_PCT + 1) as u64,
            ) as i64;
        let roll = price * pct / 100;
        let amount = if self.view.current == self.me {
            if self.cash_after(price) >= 0 {
                roll.max(price).min(self.cash())
            } else {
                0
            }
        } else {
            let bid = roll.min(self.cash_after(RESERVE));
            if bid >= price * MIN_BID_PCT / 100 {
                bid
            } else {
                0
            }
        };
        Some(CommandKind::SubmitBlindBid { amount })
    }

    /// Picks a movement card by scoring the tile it would land on
    /// (ADR-0017): an affordable unowned property is worth pursuing, an
    /// expensive rival-owned tile is worth avoiding, `GoToJail` strongly
    /// so; everything else is roughly neutral. Ties break to the lowest
    /// value - simple and deterministic.
    fn choose_movement_card(&self) -> Option<CommandKind> {
        let len = self.content.board.len();
        let from = self.view.players[self.me].position;
        let value = *self
            .view
            .players
            .get(self.me)?
            .hand
            .iter()
            .max_by_key(|&&v| (self.landing_score(from, v, len), std::cmp::Reverse(v)))?;
        Some(CommandKind::PlayMovementCard { value })
    }

    fn landing_score(&self, from: usize, value: u8, len: usize) -> i64 {
        let to = (from + value as usize) % len;
        match &self.content.board[to].kind {
            TileKind::GoToJail => SCORE_GO_TO_JAIL,
            TileKind::Property(prop) => match self.view.tiles[to].owner {
                None => {
                    if self.cash_after(prop.price) >= RESERVE {
                        SCORE_BUYABLE_UNOWNED
                    } else {
                        0
                    }
                }
                Some(o) if o == self.me => SCORE_OWN_TILE,
                // Cheap rival tiles are a shrug; expensive ones are worth
                // steering away from when there's a choice.
                Some(_) => -(prop.price * RIVAL_TILE_PENALTY_PCT / 100),
            },
            _ => SCORE_NEUTRAL,
        }
    }

    /// Jail triage (ADR-0024): the jail card first if held (simplest, no
    /// freeze, no vote); a bribe when comfortably richer than twice the
    /// reserve it risks; otherwise the safe default, Legal Route in
    /// ascending order.
    fn jail_action(&self) -> CommandKind {
        if self.view.players[self.me].jail_cards > 0 {
            return CommandKind::UseJailCard;
        }
        let bribe = RESERVE * 2;
        if self.cash_after(bribe) >= RESERVE * 2 {
            return CommandKind::OfferBribe { amount: bribe };
        }
        let order: Vec<u8> =
            (self.content.rules.velocity_min..=self.content.rules.velocity_max).collect();
        CommandKind::ChooseLegalRoute { order }
    }

    /// Accepts a bribe when the per-head payout is material (at least half
    /// the usual reserve); never votes on its own bribe or twice.
    fn bribe_vote_action(
        &self,
        briber: usize,
        amount: i64,
        votes: &[Option<bool>],
    ) -> Option<CommandKind> {
        if self.me == briber || votes[self.me].is_some() {
            return None;
        }
        let opponents = self
            .view
            .players
            .iter()
            .enumerate()
            .filter(|&(i, p)| i != briber && !p.bankrupt)
            .count()
            .max(1);
        let share = amount / opponents as i64;
        Some(CommandKind::VoteOnBribe {
            accept: share >= RESERVE / 2,
        })
    }

    fn asset_action(&self) -> Option<CommandKind> {
        self.sell_house_for_liquidity()
            .or_else(|| self.mortgage_for_liquidity())
            .or_else(|| self.unmortgage_best_tile())
            .or_else(|| self.build_best_tile())
            .or_else(|| self.expropriate_group_completer())
            .or_else(|| self.boost_best_tile())
    }

    fn sell_house_for_liquidity(&self) -> Option<CommandKind> {
        if self.cash() >= RESERVE {
            return None;
        }
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let TileKind::Property(prop) = &self.content.board[i].kind else {
                    return false;
                };
                self.view.tiles[i].houses > 0 && self.can_sell_evenly(i, &prop.group)
            })
            .max_by_key(|&(i, _)| self.view.tiles[i].houses)?;
        Some(CommandKind::SellHouse { tile: id })
    }

    fn mortgage_for_liquidity(&self) -> Option<CommandKind> {
        if self.cash() >= RESERVE {
            return None;
        }
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let TileKind::Property(prop) = &self.content.board[i].kind else {
                    return false;
                };
                !self.view.tiles[i].mortgaged && self.group_has_no_houses(&prop.group)
            })
            .max_by_key(|&(i, _)| self.content.property(i).map_or(0, |p| p.price))?;
        Some(CommandKind::Mortgage { tile: id })
    }

    fn unmortgage_best_tile(&self) -> Option<CommandKind> {
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let Some(prop) = self.content.property(i) else {
                    return false;
                };
                let cost = mortgage_redeem_cost(prop.price);
                self.view.tiles[i].mortgaged && self.cash_after(cost) >= REDEEM_RESERVE
            })
            .max_by_key(|&(i, _)| self.tile_priority(i))?;
        Some(CommandKind::Unmortgage { tile: id })
    }

    fn build_best_tile(&self) -> Option<CommandKind> {
        let cap = self.content.rules.max_houses_per_property.min(5);
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let Some(prop) = self.content.property(i) else {
                    return false;
                };
                prop.rent_model == RentModel::Houses
                    && self.view.tiles[i].houses < cap
                    && self.cash_after(prop.house_cost) >= BUILD_RESERVE
                    && self.owns_full_clean_group(&prop.group)
                    && self.can_build_evenly(i, &prop.group)
            })
            .max_by_key(|&(i, _)| self.tile_priority(i))?;
        Some(CommandKind::Build { tile: id })
    }

    fn boost_best_tile(&self) -> Option<CommandKind> {
        if self.content.rules.rent_boost <= 0 {
            return None;
        }
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let Some(prop) = self.content.property(i) else {
                    return false;
                };
                let cost = prop.price * self.content.rules.rent_boost / 100;
                !self.view.tiles[i].mortgaged
                    && self.view.tiles[i].boosts < MAX_RENT_BOOSTS
                    && self.cash_after(cost) >= BUILD_RESERVE
                    && self.tile_base_rent(i) > 0
            })
            .max_by_key(|&(i, _)| self.tile_priority(i))?;
        Some(CommandKind::BoostRent { tile: id })
    }

    /// Takeover only applies to the tile the bot just landed on (ADR-0022),
    /// so unlike the other asset actions this checks a single tile instead
    /// of scanning the board.
    fn expropriate_group_completer(&self) -> Option<CommandKind> {
        if self.content.rules.expropriation <= 0 || !matches!(self.view.turn, TurnPhase::AwaitEnd) {
            return None;
        }
        let i = self.view.players[self.me].position;
        let def = self.content.board.get(i)?;
        let prop = self.content.property(i)?;
        let owner = self.view.tiles[i].owner?;
        let cost = prop.price * self.content.rules.expropriation / 100;
        (owner != self.me
            && !self.view.players[owner].bankrupt
            && !self.view.tiles[i].mortgaged
            && self.cash_after(cost) >= BUILD_RESERVE
            && self.completes_group(i))
        .then(|| CommandKind::Expropriate {
            tile: def.id.clone(),
        })
    }

    fn owned_properties(&self) -> impl Iterator<Item = (usize, String)> + '_ {
        self.content
            .board
            .iter()
            .enumerate()
            .filter(move |&(i, _)| self.view.tiles[i].owner == Some(self.me))
            .filter_map(|(i, def)| self.content.property(i).map(|_| (i, def.id.clone())))
    }

    fn can_fulfill_trade_payment(&self, cash: i64, tiles: &[usize]) -> bool {
        self.cash() >= cash
            && tiles.iter().all(|&tile| {
                self.view
                    .tiles
                    .get(tile)
                    .is_some_and(|t| t.owner == Some(self.me))
            })
    }

    fn tiles_value(&self, tiles: &[usize]) -> i64 {
        tiles.iter().map(|&tile| self.tile_net_value(tile)).sum()
    }

    fn tile_net_value(&self, tile: usize) -> i64 {
        let Some(prop) = self.content.property(tile) else {
            return 0;
        };
        let mortgage_value = if self.view.tiles[tile].mortgaged {
            prop.price * MORTGAGE_VALUE_PCT / 100
        } else {
            prop.price
        };
        let improvement_value = i64::from(self.view.tiles[tile].houses) * prop.house_cost;
        let strategic = if self.completes_group(tile) {
            prop.price * GROUP_PREMIUM_PCT / 100
        } else {
            0
        };
        mortgage_value + improvement_value + strategic
    }

    fn tile_priority(&self, tile: usize) -> i64 {
        let Some(prop) = self.content.property(tile) else {
            return 0;
        };
        let group_bonus = if self.owns_full_group(&prop.group) {
            prop.price * GROUP_PREMIUM_PCT / 100
        } else {
            0
        };
        self.tile_base_rent(tile) + group_bonus + prop.price * PRIORITY_PRICE_PCT / 100
    }

    fn tile_base_rent(&self, tile: usize) -> i64 {
        let Some(prop) = self.content.property(tile) else {
            return 0;
        };
        match prop.rent_model {
            RentModel::Houses => prop.rents[self.view.tiles[tile].houses as usize],
            RentModel::GroupScaled => {
                let owned = self
                    .content
                    .group_tiles(&prop.group)
                    .filter(|&t| self.view.tiles[t].owner == Some(self.me))
                    .count();
                prop.rents[owned.saturating_sub(1).min(5)]
            }
        }
    }

    fn completes_group(&self, tile: usize) -> bool {
        let Some(prop) = self.content.property(tile) else {
            return false;
        };
        self.content
            .group_tiles(&prop.group)
            .all(|t| t == tile || self.view.tiles[t].owner == Some(self.me))
    }

    fn owns_full_group(&self, group: &str) -> bool {
        self.content
            .group_tiles(group)
            .all(|t| self.view.tiles[t].owner == Some(self.me))
    }

    fn owns_full_clean_group(&self, group: &str) -> bool {
        self.content
            .group_tiles(group)
            .all(|t| self.view.tiles[t].owner == Some(self.me) && !self.view.tiles[t].mortgaged)
    }

    fn group_has_no_houses(&self, group: &str) -> bool {
        self.content
            .group_tiles(group)
            .all(|t| self.view.tiles[t].houses == 0)
    }

    fn can_build_evenly(&self, tile: usize, group: &str) -> bool {
        let min = self
            .content
            .group_tiles(group)
            .map(|t| self.view.tiles[t].houses)
            .min()
            .unwrap_or(0);
        self.view.tiles[tile].houses == min
    }

    fn can_sell_evenly(&self, tile: usize, group: &str) -> bool {
        let max = self
            .content
            .group_tiles(group)
            .map(|t| self.view.tiles[t].houses)
            .max()
            .unwrap_or(0);
        self.view.tiles[tile].houses == max
    }
}

const fn mortgage_redeem_cost(price: i64) -> i64 {
    let principal = price * MORTGAGE_VALUE_PCT / 100;
    principal + principal * MORTGAGE_INTEREST_PCT / 100
}

#[cfg(test)]
mod tests;
