//! Parcello game server: authoritative, self-hostable (Minecraft model).
//!
//! Boot sequence: parse args -> resolve the server-wide mod set (ADR-0004)
//! -> bind -> serve `/ws`. Rooms are created on demand by client connections.

mod auth;
mod history;
mod room;
mod ws;

use std::path::PathBuf;
use std::sync::Arc;

use axum::response::Html;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use parcello_mods::ResolvedContent;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use auth::{CompositeVerifier, IdentityVerifier};
use history::{GameHistory, MemoryHistory, SqliteHistory};
use room::Rooms;

#[derive(Parser, Debug)]
#[command(name = "parcello-server", about = "Parcello authoritative game server")]
struct Args {
    /// Listen address.
    #[arg(long, default_value = "0.0.0.0:7878")]
    bind: String,

    /// Directory containing mod bundles (one subdirectory per mod id).
    #[arg(long, default_value = "mods")]
    mods_dir: PathBuf,

    /// Ordered mod list; later mods override earlier ones per key.
    #[arg(long = "mod", default_values_t = vec!["base".to_string()])]
    mods: Vec<String>,

    /// Accept unauthenticated guests (LAN/testing; identities are spoofable).
    #[arg(long)]
    insecure_guest: bool,

    /// SQLite file for game history (seeds + accepted-command replay logs).
    /// Omit for in-memory history.
    #[arg(long)]
    history: Option<PathBuf>,

    /// Auto-play the canonical action (roll/decline/pass/end turn) for the
    /// acting player after this many seconds without progress. 0 disables.
    #[arg(long, default_value_t = 0)]
    turn_timeout: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub rooms: Rooms,
    pub content: Arc<ResolvedContent>,
    pub verifier: Arc<dyn IdentityVerifier>,
    pub history: Arc<dyn GameHistory>,
    pub turn_timeout: Option<std::time::Duration>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let resolved = parcello_mods::resolve(&args.mods_dir, &args.mods)?;
    info!(
        mods = ?args.mods,
        tiles = resolved.content.board.len(),
        chance = resolved.content.chance.len(),
        community = resolved.content.community.len(),
        "content resolved"
    );

    let jwt_secret = std::env::var("PARCELLO_JWT_SECRET").ok();
    if jwt_secret.is_none() && !args.insecure_guest {
        warn!("no PARCELLO_JWT_SECRET and no --insecure-guest: nobody can authenticate");
    }
    if args.insecure_guest {
        warn!("--insecure-guest: guest identities are spoofable; LAN/testing only");
    }
    let verifier = Arc::new(CompositeVerifier::new(jwt_secret, args.insecure_guest));

    let history: Arc<dyn GameHistory> = match &args.history {
        Some(path) => {
            info!(path = %path.display(), "sqlite history enabled");
            Arc::new(SqliteHistory::open(path)?)
        }
        None => Arc::new(MemoryHistory::new()),
    };
    let turn_timeout = match args.turn_timeout {
        0 => None,
        secs => {
            info!(seconds = secs, "per-turn AFK timeout enabled");
            Some(std::time::Duration::from_secs(secs))
        }
    };
    let state = AppState {
        rooms: Rooms::default(),
        content: Arc::new(resolved),
        verifier,
        history,
        turn_timeout,
    };

    let app = Router::new()
        // Embedded web client: the server is the whole deployment.
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.bind).await?;
    info!(bind = %args.bind, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            info!("shutting down");
        })
        .await?;
    Ok(())
}
