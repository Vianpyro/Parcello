//! Parcello game server: authoritative, self-hostable (Minecraft model).
//!
//! Boot sequence: parse args -> resolve the server-wide mod set (ADR-0004)
//! -> bind -> serve `/ws`. Rooms are created on demand by client connections.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use parcello_server::auth::CompositeVerifier;
use parcello_server::history::{GameHistory, MemoryHistory, SqliteHistory};
use parcello_server::room::Rooms;
use parcello_server::{AppState, eddsa, game_router, lan};

#[derive(Parser, Debug)]
#[command(
    name = "parcello-server",
    version,
    about = "Parcello authoritative game server"
)]
struct Args {
    /// Listen address.
    #[arg(long, env = "PARCELLO_BIND", default_value = "0.0.0.0:7878")]
    bind: String,

    /// Directory containing mod bundles (one subdirectory per mod id).
    #[arg(long, env = "PARCELLO_MODS_DIR", default_value = "mods")]
    mods_dir: PathBuf,

    /// Directory containing the built Flutter Web client
    /// (`flutter build web --release` in clients/flutter).
    #[arg(long, env = "PARCELLO_WEB_DIR", default_value = "web")]
    web_dir: PathBuf,

    /// Ordered mod list; later mods override earlier ones per key.
    #[arg(
        long = "mod",
        env = "PARCELLO_MODS",
        value_delimiter = ',',
        default_values_t = vec!["base".to_string()]
    )]
    mods: Vec<String>,

    /// Accept unauthenticated guests (LAN/testing; identities are spoofable).
    #[arg(long, env = "PARCELLO_INSECURE_GUEST")]
    insecure_guest: bool,

    /// JWKS URL of an `EdDSA` identity provider (ADR-0009); repeatable for
    /// redundant issuer instances. Enables `id:` token logins.
    #[arg(
        long = "identity-url",
        env = "PARCELLO_IDENTITY_URLS",
        value_delimiter = ','
    )]
    identity_urls: Vec<String>,

    /// When set, identity tokens must carry this audience (`aud` claim).
    #[arg(long, env = "PARCELLO_IDENTITY_AUDIENCE")]
    identity_audience: Option<String>,

    /// `SQLite` file for game history (seeds + accepted-command replay logs).
    /// Omit for in-memory history.
    #[arg(long, env = "PARCELLO_HISTORY")]
    history: Option<PathBuf>,

    /// Default per-turn limit for new rooms (seconds): auto-play the
    /// canonical action (roll/decline/pass/end turn) for the acting player
    /// after this long without progress, unless their personal time bank
    /// covers the overage (ADR-0023). 0 disables. The host can change it per
    /// room in the lobby (ADR-0015).
    #[arg(long, env = "PARCELLO_TURN_TIMEOUT", default_value_t = 12)]
    turn_timeout: u64,

    /// Default personal time bank for new rooms (seconds): a connected
    /// acting seat may overrun `--turn-timeout` by draining this per-match
    /// reserve, never refilled (ADR-0023). 0 disables (turn limit hard-stops
    /// with no overrun). The host can change it per room in the lobby.
    #[arg(long, env = "PARCELLO_TIME_BANK", default_value_t = 45)]
    time_bank_seconds: u64,

    /// Default game length for new rooms (seconds): the game ends and the
    /// richest player (by net worth) wins (ADR-0010). 0 = untimed. The host
    /// can change it per room in the lobby (ADR-0015).
    #[arg(long, env = "PARCELLO_GAME_TIMEOUT", default_value_t = 3600)]
    game_timeout: u64,

    /// Enable LAN discovery announcements (multicast) for local network
    /// game browsing.
    #[arg(long, env = "PARCELLO_LAN")]
    lan: bool,

    /// Multicast address to announce to (default: 239.255.0.1).
    #[arg(long, env = "PARCELLO_LAN_MADDR", default_value = "239.255.0.1")]
    lan_maddr: String,

    /// Multicast port to announce to (default: 55888).
    #[arg(long, env = "PARCELLO_LAN_PORT", default_value_t = 55888)]
    lan_port: u16,

    /// Also send a broadcast fallback to 255.255.255.255:<port> when
    /// multicast delivery may be unreliable.
    #[arg(long, env = "PARCELLO_LAN_BROADCAST_FALLBACK")]
    lan_broadcast_fallback: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let web_index = args.web_dir.join("index.html");
    anyhow::ensure!(
        web_index.is_file(),
        "web dir {} has no index.html (run `flutter build web --release` \
         in clients/flutter and point --web-dir/PARCELLO_WEB_DIR at the output)",
        args.web_dir.display()
    );
    info!(dir = %args.web_dir.display(), "serving flutter web build");

    let state = build_state(&args)?;

    if args.lan {
        lan::spawn_broadcaster(
            args.lan_maddr.clone(),
            args.lan_port,
            args.lan_broadcast_fallback,
            args.bind.clone(),
        );
    }

    // Flutter Web build, served from disk (mirrors --mods-dir, ADR-0025):
    // resolved once at boot, not compiled in, so an operator can update the
    // web client without rebuilding the server binary.
    let serve_web = ServeDir::new(&args.web_dir).not_found_service(ServeFile::new(web_index));

    let app = game_router(state).fallback_service(serve_web);

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

/// Boot-time wiring: resolve the default mod set, choose the identity
/// verifier and history backend from the flags, and translate the numeric
/// timeout flags (`0` = off) into optional durations.
fn build_state(args: &Args) -> anyhow::Result<AppState> {
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

    // `0` = feature off, anything else enables it with that many seconds.
    let timeout = |secs: u64, label: &str| {
        (secs != 0).then(|| {
            info!(seconds = secs, "{label} enabled");
            std::time::Duration::from_secs(secs)
        })
    };
    Ok(AppState {
        rooms: Rooms::default(),
        content: Arc::new(resolved),
        mods_dir: Arc::new(args.mods_dir.clone()),
        verifier,
        history,
        turn_timeout: timeout(args.turn_timeout, "per-turn AFK timeout"),
        time_bank: timeout(args.time_bank_seconds, "personal time bank"),
        game_timeout: timeout(args.game_timeout, "time-boxed games (richest wins)"),
    })
}
