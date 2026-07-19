//! Parcello game server library: rooms, transport, auth, history.
//!
//! The binary (`main.rs`) only parses flags and wires this together; the
//! split exists so integration tests (and future tooling) can build the
//! same router against an in-memory `AppState`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use parcello_mods::ResolvedContent;
use serde::Serialize;
use tokio::sync::Semaphore;

/// Global ceiling on concurrent WebSocket connections.
///
/// A community-hosted server is reachable by untrusted clients; this bounds
/// how many sockets (memory + file descriptors) one server process will ever
/// hold at once. Generous vs. real play (a room is 2..=6 seats); per-IP
/// throttling is left to the reverse proxy the deployment guide puts in front
/// (docs/deployment.md).
pub const MAX_CONNECTIONS: usize = 1024;

pub mod auth;
pub mod eddsa;
pub mod history;
pub mod lan;
pub mod ranked;
pub mod room;
pub mod showcase;
pub mod ws;

use auth::IdentityVerifier;
use history::GameHistory;
use ranked::RankedService;
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
    /// OIDC issuer URL the web client pre-fills in its sign-in dialog, served
    /// at runtime via `/config.json` (ADR-0032) so an operator sets it per
    /// deployment without rebuilding the Flutter bundle. `None` = the client
    /// keeps its own generic default.
    pub default_issuer: Option<String>,
    /// Global concurrent-connection limiter (`MAX_CONNECTIONS` permits); a
    /// socket holds one permit for its whole life (ws.rs).
    pub connections: Arc<Semaphore>,
    /// Ranked matchmaking (ADR-0034): the queue plus the rating store.
    /// `None` when `--ranked` is off - every ranked message then answers
    /// with an explicit "disabled" error.
    pub ranked: Option<Arc<RankedService>>,
    /// Whether this server accepts unauthenticated guests
    /// (`--insecure-guest`). Advertised via `/config.json` (ADR-0032) so
    /// clients can hide the guest path instead of offering a login mode the
    /// server would only reject.
    pub guest_allowed: bool,
    /// Keep a bots showcase game running when no humans are playing
    /// (`--showcase`, ADR-0035), so `spectate` always finds something.
    pub showcase: bool,
}

impl AppState {
    /// A fresh connection limiter sized to `MAX_CONNECTIONS`. Every
    /// `AppState` constructor uses this so the cap is defined in one place.
    #[must_use]
    pub fn connection_limiter() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(MAX_CONNECTIONS))
    }
}

/// Runtime configuration the web client reads once at startup (ADR-0032):
/// per-deployment values an operator sets without recompiling the Flutter
/// bundle. Unset fields are omitted so the client falls back to its own
/// compile-time defaults.
#[derive(Serialize)]
struct ClientConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    default_issuer: Option<String>,
    /// Always present (both values are definitive answers): clients hide
    /// the guest sign-in path when this is false. Absent only on servers
    /// predating the field, which clients treat as "unknown - keep the
    /// guest option" for compatibility.
    guest_allowed: bool,
}

async fn client_config(State(state): State<AppState>) -> Json<ClientConfig> {
    Json(ClientConfig {
        default_issuer: state.default_issuer,
        guest_allowed: state.guest_allowed,
    })
}

/// The game-facing routes (`/healthz`, `/ws`, `/config.json`). The binary
/// layers the Flutter Web static service on top; tests use this bare router.
pub fn game_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/config.json", get(client_config))
        .route("/ws", get(ws::ws_handler))
        .with_state(state)
}
