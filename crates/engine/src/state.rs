//! Authoritative game state. Fully serializable: a `(GameState, command log)`
//! pair is sufficient to replay or audit a game.
//!
//! Hidden information (PRNG seed, deck order) lives here and must never reach
//! clients; see `view::ClientView` for the public projection.

use serde::{Deserialize, Serialize};

use crate::content::GameContent;
use crate::content::{MarketEffect, RentModel, RuleParams};
use crate::rng;
use crate::tuning::{
    FORECAST_QUEUE_LEN, MORTGAGE_VALUE_PCT, VP_PER_CONGLOMERATE, VP_PER_FULL_GROUP,
    VP_PER_GROUP_SCALED,
};

/// Global player identity issued by the identity service ("provider:sub")
/// or a guest id in insecure mode.
pub type PlayerId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameState {
    pub phase: GamePhase,
    /// Seating order, fixed at game start. Bankrupt players stay in the
    /// vector (flagged) so indices remain stable in events and views.
    pub players: Vec<Player>,
    /// Index into `players` of the acting player.
    pub current: usize,
    pub turn: TurnPhase,
    /// Dynamic tile state, parallel to `GameContent::board`.
    pub tiles: Vec<TileState>,
    pub chance_deck: DeckState,
    pub community_deck: DeckState,
    /// SplitMix64 PRNG state. Part of the state on purpose: replay-safe.
    /// Never expose to clients (dice would become predictable).
    pub rng: u64,
    /// Completed turn transitions; used for stats/history.
    pub turn_count: u32,
    /// Open trade offers, visible to all players. Asset validity is
    /// re-checked at acceptance time; stale offers reject without mutating.
    #[serde(default)]
    pub pending_trades: Vec<TradeOffer>,
    #[serde(default)]
    pub trade_seq: u32,
    /// Shared table-wide stock of subsidiary levels (ADR-0019); `None` =
    /// unlimited (`rules.subsidiary_pool_factor == 0`, the default).
    /// Computed once at `GameState::new` and never recomputed mid-game.
    #[serde(default)]
    pub subsidiaries_available: Option<u64>,
    /// Shared table-wide stock of conglomerate (top) levels; `None` =
    /// unlimited (`rules.conglomerate_pool_factor == 0`, the default).
    #[serde(default)]
    pub conglomerates_available: Option<u64>,
    /// Public market forecast queue (ADR-0021); empty/inert when the
    /// content's `market_events` pool is empty.
    #[serde(default)]
    pub forecast: MarketForecast,
    /// The property currently in the Exposition corner's spotlight
    /// (ADR-0026), if any. Deliberately a `GameState` field rather than a
    /// `TileState` one (unlike the ADR-0012 boost): a fact about the
    /// location/a table-wide event, not an owner-purchased upgrade, so it
    /// survives trades/expropriation/bankruptcy untouched.
    #[serde(default)]
    pub spotlight: Option<Spotlight>,
}

/// A standing offer: `from` gives `give_*` and receives `receive_*`.
/// Tiles transfer with their mortgage status as-is (same rule as
/// bankruptcy); groups must be house-free on both sides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TradeOffer {
    pub id: u32,
    pub from: usize,
    pub to: usize,
    pub give_cash: i64,
    pub give_tiles: Vec<usize>,
    pub receive_cash: i64,
    pub receive_tiles: Vec<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GamePhase {
    Active,
    Finished { winner: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnPhase {
    /// Waiting for the current player to play a movement card (ADR-0017),
    /// or - while jailed - to choose an exit (Legal Route, Corruption, or
    /// the jail card).
    AwaitMove,
    /// Landed on an unowned property: a 5s sealed-bid window is open
    /// (ADR-0018). One slot per seat, parallel to `players`; `None` = not
    /// yet submitted. The landing player (`GameState::current`, stable for
    /// the whole window - see the turn-advance guard in `apply.rs`) is the
    /// discoverer and gets an implicit list-price floor bid.
    BlindAuction { tile: usize, bids: Vec<Option<i64>> },
    /// A jailed player offered a bribe (ADR-0024): a 5s simultaneous vote
    /// among living opponents. One slot per seat, parallel to `players`;
    /// `None` = not yet voted; the briber never votes on their own offer.
    BribeVote {
        briber: usize,
        amount: i64,
        votes: Vec<Option<bool>>,
    },
    /// Movement resolved; building allowed; waiting for end of turn.
    AwaitEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub cash: i64,
    pub position: usize,
    /// Whether this player is in jail. Escape is a choice, not a roll
    /// (ADR-0024: Legal Route, Corruption, or the jail card) - no more
    /// failed-attempt counter, forced fine, or third-roll rule.
    pub jailed: bool,
    /// Get-out-of-jail-free cards held. A count, not card identities: the
    /// decks are immutable cyclic shuffles, so drawn cards never leave the
    /// rotation (documented simplification).
    #[serde(default)]
    pub jail_cards: u8,
    pub bankrupt: bool,
    /// Movement values currently held (ADR-0017's velocity deck); public
    /// like cash. Refills to `velocity_min..=velocity_max` the instant it
    /// empties (see `Exec::maybe_refill_hand`), which is also the single
    /// `hands_cycled` tick below.
    #[serde(default)]
    pub hand: Vec<u8>,
    /// `Some(queue)` while serving a locked, public Legal Route (ADR-0024):
    /// `queue[0]` is the only card `PlayMovementCard` will accept next.
    /// While `Some`, this player's owned tiles charge no rent to whoever
    /// lands on them - visitors play free (`resolve_landing` checks the
    /// tile owner's `jail_route`, independent of whose turn is resolving).
    /// `None` otherwise.
    #[serde(default)]
    pub jail_route: Option<Vec<u8>>,
    /// Hands fully cycled (ADR-0020's round metronome): incremented once
    /// per hand refill, i.e. roughly once every `hand` size turns, not
    /// once per turn.
    #[serde(default)]
    pub hands_cycled: u32,
    /// Permanent victory points banked from round-bonus wins (ADR-0020);
    /// the only non-reversible term in `GameState::victory_points`.
    #[serde(default)]
    pub round_bonus_vp: i64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TileState {
    /// Index into `players`, if owned.
    pub owner: Option<usize>,
    /// 0..=5, where 5 renders as a hotel.
    pub houses: u8,
    /// Mortgaged tiles collect no rent; ownership still counts for groups.
    #[serde(default)]
    pub mortgaged: bool,
    /// Rent-boost level (ADR-0012): each step raises this tile's rent by a
    /// fixed percent. Reset when the tile changes hands.
    #[serde(default)]
    pub boosts: u8,
}

/// Cyclic deck: cards are drawn in shuffled order and recycled without
/// reshuffling (deterministic and sufficient for the base game).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeckState {
    pub order: Vec<u16>,
    pub next: usize,
}

impl DeckState {
    fn shuffled(len: usize, rng: &mut u64) -> Self {
        let mut order: Vec<u16> = (0..len as u16).collect();
        rng::shuffle(&mut order, rng);
        Self { order, next: 0 }
    }

    /// Returns the content index of the next card and advances the cursor.
    pub fn draw(&mut self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        let card = self.order[self.next] as usize;
        self.next = (self.next + 1) % self.order.len();
        Some(card)
    }
}

/// A drawn-but-not-yet-active market event (ADR-0021): public the moment
/// it's scheduled, so players can plan around it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub event_id: String,
    pub starts_at_turn: u32,
    pub duration: u32,
}

/// The market event currently in effect, if any (ADR-0021). Only
/// `RentMultiplier`/`AcquisitionMultiplier` ever occupy this - `WealthTax`
/// resolves instantly the moment it activates and never lingers here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveMarketEvent {
    pub event_id: String,
    pub effect: MarketEffect,
    pub magnitude_pct: i64,
    pub ends_at_turn: u32,
}

/// Public market forecast queue (ADR-0021): the next scheduled events plus
/// whichever one is currently in effect. Empty and permanently inert when
/// the content's `market_events` pool is empty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MarketForecast {
    /// Upcoming events, oldest (soonest) first, kept at 3 entries.
    pub queue: Vec<ScheduledEvent>,
    pub active: Option<ActiveMarketEvent>,
}

/// The property currently in the Exposition corner's spotlight (ADR-0026):
/// its rent is boosted until `expires_at_turn`. Public in `ClientView`
/// unconditionally - the whole point is that the table sees the hot tile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Spotlight {
    pub tile: usize,
    pub expires_at_turn: u32,
}

impl MarketForecast {
    /// Draws one event from `content.market_events` and schedules it after
    /// whatever is already queued (or after `now` if the queue is empty),
    /// `content.forecast_gap_turns` later. A complete no-op - no RNG draw -
    /// when the pool is empty, so mods without `events.toml` never perturb
    /// the seeded RNG stream. Used both to seed the initial 3 events and to
    /// refill the queue each time one activates.
    pub(crate) fn draw_next(&mut self, content: &GameContent, rng: &mut u64, now: u32) {
        if content.market_events.is_empty() {
            return;
        }
        let idx = rng::below(rng, content.market_events.len() as u64) as usize;
        let def = &content.market_events[idx];
        let after = self.queue.last().map_or(now, |s| s.starts_at_turn);
        self.queue.push(ScheduledEvent {
            event_id: def.id.clone(),
            starts_at_turn: after + content.forecast_gap_turns,
            duration: def.duration_turns,
        });
    }
}

impl GameState {
    pub(crate) fn new(
        content: &GameContent,
        players: Vec<(PlayerId, String)>,
        seed: u64,
        rules: &RuleParams,
    ) -> Self {
        assert!(players.len() >= 2, "a game requires at least two players");
        let player_count = players.len();
        let mut rng = seed;
        let chance_deck = DeckState::shuffled(content.chance.len(), &mut rng);
        let community_deck = DeckState::shuffled(content.community.len(), &mut rng);
        // round(factor * sqrt(players)); 0 disables pooling (unlimited stock,
        // the off-by-default idiom shared with expropriation/rent_boost).
        let pool_size = |factor: i64| -> Option<u64> {
            (factor > 0).then(|| (factor as f64 * (player_count as f64).sqrt()).round() as u64)
        };
        // Seed the public forecast with 3 events, gap_turns apart (ADR-0021);
        // a no-op loop when the content ships no market events.
        let mut forecast = MarketForecast::default();
        for _ in 0..FORECAST_QUEUE_LEN {
            forecast.draw_next(content, &mut rng, 0);
        }
        // First player drawn from the seed (2026-07 playtest decision), not
        // hardwired to the host's seat 0 - deterministic and replay-safe
        // like every other draw. Turn order itself stays the seating order.
        let first = rng::below(&mut rng, player_count as u64) as usize;
        let full_hand: Vec<u8> = (rules.velocity_min..=rules.velocity_max).collect();
        Self {
            phase: GamePhase::Active,
            players: players
                .into_iter()
                .map(|(id, name)| Player {
                    id,
                    name,
                    cash: rules.starting_balance,
                    position: 0,
                    jailed: false,
                    jail_cards: 0,
                    bankrupt: false,
                    hand: full_hand.clone(),
                    jail_route: None,
                    hands_cycled: 0,
                    round_bonus_vp: 0,
                })
                .collect(),
            current: first,
            turn: TurnPhase::AwaitMove,
            tiles: vec![TileState::default(); content.board.len()],
            chance_deck,
            community_deck,
            rng,
            turn_count: 0,
            pending_trades: Vec::new(),
            trade_seq: 0,
            subsidiaries_available: pool_size(rules.subsidiary_pool_factor),
            conglomerates_available: pool_size(rules.conglomerate_pool_factor),
            forecast,
            spotlight: None,
        }
    }

    /// Draws the net-worth tax bracket for an audit tile landing
    /// (ADR-0029): a percent in `min_pct..=max_pct` with linearly
    /// decreasing weight, so the heaviest bracket is the rarest (weight
    /// of `p` is `max_pct - p + 1`; e.g. for 5..=25, 5% is 21x more
    /// likely than 25%). Validated content guarantees `min <= max`.
    pub(crate) fn draw_networth_tax_pct(&mut self, min_pct: u8, max_pct: u8) -> u8 {
        let (min, max) = (min_pct as u64, max_pct as u64);
        // Total weight of the descending triangle min..=max.
        let n = max - min + 1;
        let total: u64 = (1..=n).sum();
        let mut r = rng::below(&mut self.rng, total);
        for pct in min..=max {
            let weight = max - pct + 1;
            if r < weight {
                return pct as u8;
            }
            r -= weight;
        }
        min_pct // unreachable with a correct total; safe fallback
    }

    /// Draws a uniformly random property tile via the seeded RNG
    /// (ADR-0026), for the Exposition corner's landing draw. `None` when
    /// the board has no property tiles (mod-broken content degrades to a
    /// no-op, matching `DeckState::draw`'s empty-deck handling).
    pub(crate) fn draw_spotlight_tile(&mut self, content: &GameContent) -> Option<usize> {
        let candidates = content.property_tiles();
        if candidates.is_empty() {
            return None;
        }
        let idx = rng::below(&mut self.rng, candidates.len() as u64) as usize;
        Some(candidates[idx])
    }

    pub fn alive_players(&self) -> impl Iterator<Item = usize> + '_ {
        self.players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.bankrupt)
            .map(|(i, _)| i)
    }

    /// True when `player` owns every tile of `group` (monopoly).
    pub fn owns_full_group(&self, content: &GameContent, player: usize, group: &str) -> bool {
        let tiles = content.group_tiles(group);
        !tiles.is_empty() && tiles.iter().all(|&t| self.tiles[t].owner == Some(player))
    }

    /// Number of distinct colour groups `player` owns completely (ADR-0013).
    /// Mortgaged tiles still count for ownership.
    pub fn full_groups_owned(&self, content: &GameContent, player: usize) -> usize {
        let mut groups: Vec<&str> = content
            .board
            .iter()
            .filter_map(|t| match &t.kind {
                crate::content::TileKind::Property(p) => Some(p.group.as_str()),
                _ => None,
            })
            .collect();
        groups.sort_unstable();
        groups.dedup();
        groups
            .iter()
            .filter(|g| self.owns_full_group(content, player, g))
            .count()
    }

    /// Race-to-target score (ADR-0020): 3 per complete colour group, 2 per
    /// conglomerate-level tile (`houses == max_houses_per_property`), 1 per
    /// group-scaled ("utility") tile owned, plus the stored round bonus.
    /// Fully reversible except the round bonus - lose the group/tile, lose
    /// the points.
    pub fn victory_points(&self, content: &GameContent, player: usize) -> i64 {
        let cap = content.rules.max_houses_per_property.min(5);
        let mut points = VP_PER_FULL_GROUP * self.full_groups_owned(content, player) as i64;
        for (i, tile) in self.tiles.iter().enumerate() {
            if tile.owner != Some(player) {
                continue;
            }
            let Some(prop) = content.property(i) else {
                continue;
            };
            if tile.houses >= cap {
                points += VP_PER_CONGLOMERATE;
            }
            if prop.rent_model == RentModel::GroupScaled {
                points += VP_PER_GROUP_SCALED;
            }
        }
        points + self.players[player].round_bonus_vp
    }

    /// Total assets of `player`: cash plus property equity. An unmortgaged
    /// property counts its full price; a mortgaged one counts price/2 (the
    /// owner already took the other half in cash, so mortgaging is net-worth
    /// neutral); each house counts its build cost. Clients mirror this to
    /// rank players in timed games - keep the two in step. See
    /// `docs/business-tour-direction.md`.
    pub fn net_worth(&self, content: &GameContent, player: usize) -> i64 {
        let mut worth = self.players[player].cash;
        for (i, tile) in self.tiles.iter().enumerate() {
            if tile.owner != Some(player) {
                continue;
            }
            if let Some(prop) = content.property(i) {
                worth += if tile.mortgaged {
                    prop.price * MORTGAGE_VALUE_PCT / 100
                } else {
                    prop.price
                };
                worth += tile.houses as i64 * prop.house_cost;
            }
        }
        worth
    }

    // -- Shared building pools (ADR-0019) --------------------------------
    //
    // `None` means the pool is disabled (unlimited stock); `Some(n)` is the
    // live remaining count. Shared by `apply.rs` and `strategy.rs` so
    // neither duplicates the `Option`-is-unlimited matching.

    /// Whether `n` subsidiary units could be taken right now (an unlimited
    /// pool always answers yes) - used to decide whether stepping a tile
    /// down off the top level can proceed normally.
    pub(crate) fn subsidiaries_free(&self, n: u64) -> bool {
        self.subsidiaries_available.is_none_or(|avail| avail >= n)
    }

    /// Takes one subsidiary from the pool; `Err(())` only when the pool is
    /// enabled and empty.
    pub(crate) fn take_subsidiary(&mut self) -> Result<(), ()> {
        match &mut self.subsidiaries_available {
            Some(0) => Err(()),
            Some(n) => {
                *n -= 1;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Takes one conglomerate from the pool; `Err(())` only when the pool
    /// is enabled and empty.
    pub(crate) fn take_conglomerate(&mut self) -> Result<(), ()> {
        match &mut self.conglomerates_available {
            Some(0) => Err(()),
            Some(n) => {
                *n -= 1;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Returns `n` subsidiaries to the pool; always succeeds (a pool return
    /// can never fail, only a take can).
    pub(crate) fn return_subsidiaries(&mut self, n: u64) {
        if let Some(avail) = &mut self.subsidiaries_available {
            *avail += n;
        }
    }

    /// Returns one conglomerate to the pool; always succeeds.
    pub(crate) fn return_conglomerate(&mut self) {
        if let Some(avail) = &mut self.conglomerates_available {
            *avail += 1;
        }
    }

    /// Consumes `n` subsidiaries already confirmed free via
    /// `subsidiaries_free` - the re-issue half of stepping a tile down off
    /// the top level. Never call this without checking first: unlike
    /// `take_subsidiary`, it has no failure path and will saturate rather
    /// than reject an unchecked over-consumption.
    pub(crate) fn consume_subsidiaries(&mut self, n: u64) {
        if let Some(avail) = &mut self.subsidiaries_available {
            *avail = avail.saturating_sub(n);
        }
    }

    /// Releases whatever pool units a tile currently holds at `houses`
    /// levels (of `cap` total): one conglomerate at the top level,
    /// otherwise that many subsidiaries; a no-op at zero. Always succeeds -
    /// used wherever a tile's buildings vanish outright (bankruptcy wipe,
    /// takeover liquidation, forced-liquidation full strip) rather than
    /// stepping down one level at a time.
    pub(crate) fn release_tile_pools(&mut self, houses: u8, cap: u8) {
        if houses == 0 {
            return;
        }
        if houses == cap {
            self.return_conglomerate();
        } else {
            self.return_subsidiaries(houses as u64);
        }
    }
}
