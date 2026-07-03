//! Public projection of `GameState` pushed to clients. Deliberately excludes
//! the PRNG seed and deck order: exposing either would make dice and card
//! draws predictable. Cash is public, as in the reference game.

use serde::{Deserialize, Serialize};

use crate::state::{GamePhase, GameState, TileState, TradeOffer, TurnPhase};

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerView {
    pub id: String,
    pub name: String,
    pub cash: i64,
    pub position: usize,
    pub in_jail: bool,
    pub bankrupt: bool,
}

impl ClientView {
    pub fn of(state: &GameState) -> Self {
        Self {
            phase: state.phase,
            players: state
                .players
                .iter()
                .map(|p| PlayerView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    cash: p.cash,
                    position: p.position,
                    in_jail: p.jail_turns.is_some(),
                    bankrupt: p.bankrupt,
                })
                .collect(),
            current: state.current,
            turn: state.turn,
            tiles: state.tiles.clone(),
            turn_count: state.turn_count,
            pending_trades: state.pending_trades.clone(),
        }
    }
}
