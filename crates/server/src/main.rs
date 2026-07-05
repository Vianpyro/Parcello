//! Parcello game server: authoritative, self-hostable (Minecraft model).
//!
//! Boot sequence: parse args -> resolve the server-wide mod set (ADR-0004)
//! -> bind -> serve `/ws`. Rooms are created on demand by client connections.

mod auth;
mod eddsa;
mod history;
mod lan;
mod room;
mod ws;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::response::Html;
use axum::routing::get;
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

    /// JWKS URL of an EdDSA identity provider (ADR-0009); repeatable for
    /// redundant issuer instances. Enables `id:` token logins.
    #[arg(long = "identity-url")]
    identity_urls: Vec<String>,

    /// When set, identity tokens must carry this audience (`aud` claim).
    #[arg(long)]
    identity_audience: Option<String>,

    /// SQLite file for game history (seeds + accepted-command replay logs).
    /// Omit for in-memory history.
    #[arg(long)]
    history: Option<PathBuf>,

    /// Default per-turn limit for new rooms (seconds): auto-play the
    /// canonical action (roll/decline/pass/end turn) for the acting player
    /// after this long without progress. 0 disables. The host can change it
    /// per room in the lobby (ADR-0015).
    #[arg(long, default_value_t = 25)]
    turn_timeout: u64,

    /// Default game length for new rooms (seconds): the game ends and the
    /// richest player (by net worth) wins (ADR-0010). 0 = untimed. The host
    /// can change it per room in the lobby (ADR-0015).
    #[arg(long, default_value_t = 3600)]
    game_timeout: u64,

    /// Enable LAN discovery announcements (multicast) for local network
    /// game browsing.
    #[arg(long)]
    lan: bool,

    /// Multicast address to announce to (default: 239.255.0.1).
    #[arg(long, default_value = "239.255.0.1")]
    lan_maddr: String,

    /// Multicast port to announce to (default: 55888).
    #[arg(long, default_value_t = 55888)]
    lan_port: u16,

    /// Also send a broadcast fallback to 255.255.255.255:<port> when
    /// multicast delivery may be unreliable.
    #[arg(long)]
    lan_broadcast_fallback: bool,
}

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
    pub game_timeout: Option<std::time::Duration>,
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
    let eddsa = if args.identity_urls.is_empty() {
        None
    } else {
        info!(urls = ?args.identity_urls, "EdDSA identity provider enabled");
        Some(eddsa::EdDsaVerifier::spawn(
            args.identity_urls.clone(),
            args.identity_audience.clone(),
        ))
    };
    if jwt_secret.is_some() {
        warn!("PARCELLO_JWT_SECRET (HS256) is deprecated; move to --identity-url (ADR-0009)");
    }
    if eddsa.is_none() && jwt_secret.is_none() && !args.insecure_guest {
        warn!(
            "no --identity-url, no PARCELLO_JWT_SECRET, no --insecure-guest: nobody can authenticate"
        );
    }
    if args.insecure_guest {
        warn!("--insecure-guest: guest identities are spoofable; LAN/testing only");
    }
    let verifier = Arc::new(CompositeVerifier::new(
        eddsa,
        jwt_secret,
        args.insecure_guest,
    ));

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
    let game_timeout = match args.game_timeout {
        0 => None,
        secs => {
            info!(seconds = secs, "time-boxed games enabled (richest wins)");
            Some(std::time::Duration::from_secs(secs))
        }
    };
    let state = AppState {
        rooms: Rooms::default(),
        content: Arc::new(resolved),
        mods_dir: Arc::new(args.mods_dir),
        verifier,
        history,
        turn_timeout,
        game_timeout,
    };

    if args.lan {
        lan::spawn_broadcaster(
            args.lan_maddr.clone(),
            args.lan_port,
            args.lan_broadcast_fallback,
            args.bind.clone(),
        );
    }

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
