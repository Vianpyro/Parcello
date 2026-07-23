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
use parcello_server::ranked::{
    MemoryRatings, RankedConfig, RankedService, RatingStore, SqliteRatings,
};
use parcello_server::room::Rooms;
use parcello_server::{AppState, eddsa, game_router, lan, ranked};

#[derive(Parser, Debug)]
#[command(
    name = "parcello-server",
    version,
    about = "Parcello authoritative game server"
)]
// A CLI flag surface is data-shaped: each bool IS an independent on/off
// switch (clap derives them from the struct), not a state machine in
// disguise - grouping them into enums would only obscure --help.
#[allow(clippy::struct_excessive_bools)]
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

    /// OIDC issuer URL the web client pre-fills in its sign-in dialog. Served
    /// at runtime via `/config.json` (ADR-0032), so changing it needs no
    /// rebuild of the Flutter bundle. Unset leaves the client's generic
    /// default; usually your own issuer, e.g. `https://auth.example.com`.
    #[arg(long, env = "PARCELLO_DEFAULT_ISSUER")]
    default_issuer: Option<String>,

    /// `SQLite` file for game history (seeds + accepted-command replay logs).
    /// Omit for in-memory history.
    #[arg(long, env = "PARCELLO_HISTORY")]
    history: Option<PathBuf>,

    /// Default per-turn limit for new rooms (seconds): auto-play the
    /// canonical action (movement card / Legal Route / end turn) for the acting player
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

    /// Enable ranked matchmaking with a per-server ladder (ADR-0034):
    /// token-authenticated players queue with `queue_ranked`, the server
    /// forms tables and rates results (Weng-Lin). Ratings persist in the
    /// `--history` database; without one they are in-memory and reset at
    /// restart.
    #[arg(long, env = "PARCELLO_RANKED")]
    ranked: bool,

    /// Keep a bots showcase game running whenever no humans are playing
    /// (ADR-0035), so `spectate` always finds something to watch.
    #[arg(long, env = "PARCELLO_SHOWCASE")]
    showcase: bool,

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
    ranked::spawn_matchmaker(state.clone());
    parcello_server::showcase::spawn_showcase(state.clone());

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
        // Parcello authenticates with the OIDC ID token (ADR-0009
        // amendment 2), whose `aud` is the CLIENT id - it is the only
        // claim saying the token was minted for Parcello at all. Without
        // this check the server accepts any EdDSA token the issuer signed,
        // including ones minted for a completely different application
        // that happens to share the issuer.
        if args.identity_audience.is_none() {
            warn!(
                "--identity-url without --identity-audience: any token this issuer signs is \
                 accepted, including tokens minted for other applications (ADR-0009)"
            );
        }
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
    let has_token_auth = eddsa.is_some() || jwt_secret.is_some();
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

    // Ranked matchmaking (ADR-0034): ratings share the history database
    // (separate connection, WAL) when one is configured.
    let ranked = if args.ranked {
        let store: Arc<dyn RatingStore> = if let Some(path) = &args.history {
            Arc::new(SqliteRatings::open(path)?)
        } else {
            warn!("--ranked without --history: ratings are in-memory and reset at restart");
            Arc::new(MemoryRatings::new())
        };
        info!("ranked matchmaking enabled");
        if !has_token_auth {
            warn!("--ranked needs token identities (--identity-url); guests cannot queue");
        }
        Some(RankedService::new(store, RankedConfig::default()))
    } else {
        None
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
        default_issuer: args.default_issuer.clone(),
        connections: AppState::connection_limiter(),
        ranked,
        guest_allowed: args.insecure_guest,
        showcase: args.showcase,
    })
}
