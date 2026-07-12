//! Public projection of `GameState` pushed to clients. Deliberately excludes
//! the PRNG seed and deck order: exposing either would make dice and card
//! draws predictable. Cash is public, as in the reference game; trade
//! offers are visible only to their two parties (ADR-0007), so the session
//! layer builds one view per seat with `for_seat`.

use serde::{Deserialize, Serialize};

use crate::content::GameContent;
use crate::state::{
    GamePhase, GameState, MarketForecast, Spotlight, TileState, TradeOffer, TurnPhase,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientView {
    pub phase: GamePhase,
    pub players: Vec<PlayerView>,
    pub current: usize,
    pub turn: TurnPhase,
    pub tiles: Vec<TileState>,
    pub turn_count: u32,
    #[serde(default)]
    pub pending_trades: Vec<TradeOffer>,
    /// Shared building pools (ADR-0019), public so "everyone watches the
    /// shelf empty"; `None` = unlimited (pooling disabled).
    #[serde(default)]
    pub subsidiaries_available: Option<u64>,
    #[serde(default)]
    pub conglomerates_available: Option<u64>,
    /// Public market forecast queue (ADR-0021) - reveals draws already
    /// made, never the generator (seed/deck order stay hidden).
    #[serde(default)]
    pub forecast: MarketForecast,
    /// The property currently in the Exposition corner's spotlight
    /// (ADR-0026), if any - fully public, no per-seat masking (the whole
    /// point is that the table sees the hot tile).
    #[serde(default)]
    pub spotlight: Option<Spotlight>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerView {
    pub id: String,
    pub name: String,
    pub cash: i64,
    pub position: usize,
    pub in_jail: bool,
    /// Held get-out-of-jail-free cards (public, like cash).
    #[serde(default)]
    pub jail_cards: u8,
    pub bankrupt: bool,
    /// Race-to-target score (ADR-0020); see `GameState::victory_points`.
    /// Meaningless (always 0) when `rules.win_victory_points` is off.
    #[serde(default)]
    pub victory_points: i64,
    /// Movement values currently held (ADR-0017); public like cash, never
    /// masked.
    #[serde(default)]
    pub hand: Vec<u8>,
    /// `Some(queue)` while serving a locked, public Legal Route (ADR-0024) -
    /// transparency is the price of the immediate exit and rent freeze.
    #[serde(default)]
    pub jail_route: Option<Vec<u8>>,
    /// Hands fully cycled (ADR-0020's round metronome). Public so clients
    /// can show round progress: the round number is the MINIMUM of this
    /// across surviving players, and the `+2` round bonus fires the moment
    /// the last straggler refills and lifts that minimum. Without it the
    /// bonus looked like it arrived out of nowhere (2026-07 playtest).
    #[serde(default)]
    pub hands_cycled: u32,
}

impl ClientView {
    /// Projection for one seat: everything public plus only the trade
    /// offers this seat proposed or received.
    pub fn for_seat(state: &GameState, content: &GameContent, seat: usize) -> Self {
        let mut view = Self::of(state, content);
        view.pending_trades
            .retain(|t| t.from == seat || t.to == seat);
        // Sealed-bid secrecy (ADR-0018): a seat sees only its own bid while
        // the window is open. "Who has bid" (not the amount) is covered by
        // the amount-less `Event::BlindBidSubmitted` stream instead.
        if let TurnPhase::BlindAuction { bids, .. } = &mut view.turn {
            for (i, b) in bids.iter_mut().enumerate() {
                if i != seat {
                    *b = None;
                }
            }
        }
        // Bribe vote secrecy (ADR-0024): "individual votes stay secret" -
        // same masking, same reasoning as sealed-bid amounts above.
        if let TurnPhase::BribeVote { votes, .. } = &mut view.turn {
            for (i, v) in votes.iter_mut().enumerate() {
                if i != seat {
                    *v = None;
                }
            }
        }
        view
    }

    /// Omniscient projection (every open offer). Test/replay tooling only:
    /// the server must always send `for_seat` views.
    pub fn of(state: &GameState, content: &GameContent) -> Self {
        Self {
            phase: state.phase,
            players: state
                .players
                .iter()
                .enumerate()
                .map(|(i, p)| PlayerView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    cash: p.cash,
                    position: p.position,
                    in_jail: p.jailed,
                    jail_cards: p.jail_cards,
                    bankrupt: p.bankrupt,
                    victory_points: state.victory_points(content, i),
                    hand: p.hand.clone(),
                    jail_route: p.jail_route.clone(),
                    hands_cycled: p.hands_cycled,
                })
                .collect(),
            current: state.current,
            turn: state.turn.clone(),
            tiles: state.tiles.clone(),
            turn_count: state.turn_count,
            pending_trades: state.pending_trades.clone(),
            subsidiaries_available: state.subsidiaries_available,
            conglomerates_available: state.conglomerates_available,
            forecast: state.forecast.clone(),
            spotlight: state.spotlight,
        }
    }
}
