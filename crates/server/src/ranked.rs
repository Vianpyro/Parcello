//! Ranked matchmaking with a per-server ladder (ADR-0034).
//!
//! Session-layer only: the engine, the replay format and the game rules are
//! untouched. `ladder` is the pure math (Weng-Lin ratings, placement
//! derivation), `store` the persistence port (Repository pattern, like
//! `GameHistory` but read-modify-write), `queue` the waiting pool and the
//! matchmaker task that turns it into ranked rooms.

pub mod ladder;
pub mod queue;
pub mod store;

pub use queue::{RankedConfig, RankedService, spawn_matchmaker};
pub use store::{MemoryRatings, RatingStore, SqliteRatings};
