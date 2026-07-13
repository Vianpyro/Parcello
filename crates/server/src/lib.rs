//! Parcello game server library: rooms, transport, auth, history.
//!
//! The binary (`main.rs`) only parses flags and wires this together; the
//! split exists so integration tests (and future tooling) can build the
//! same router against an in-memory `AppState`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use parcello_mods::ResolvedContent;

pub mod auth;
pub mod eddsa;
pub mod history;
pub mod lan;
pub mod room;
pub mod ws;

use auth::IdentityVerifier;
use history::GameHistory;
use room::Rooms;

/// Everything a connection handler needs, cheap to clone per request.
#[derive(Clone)]
pub struct AppState {
    pub rooms: Rooms,
    /// Default content (boot-time `--mod` list); rooms may override at
    /// creation with their own mod list (ADR-0006).
    pub content: Arc<ResolvedContent>,
    pub mods_dir: Arc<PathBuf>,
    pub verifier: Arc<dyn IdentityVerifier>,
    pub history: Arc<dyn GameHistory>,
    /// Default timers for new rooms; the host overrides them per room in the
    /// lobby (ADR-0015). `None` = disabled by default.
    pub turn_timeout: Option<std::time::Duration>,
    /// Default personal time bank for new rooms (ADR-0023). `None` = off.
    pub time_bank: Option<std::time::Duration>,
    pub game_timeout: Option<std::time::Duration>,
}

/// The game-facing routes (`/healthz`, `/ws`). The binary layers the
/// Flutter Web static service on top; tests use this bare router.
pub fn game_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws::ws_handler))
        .with_state(state)
}
