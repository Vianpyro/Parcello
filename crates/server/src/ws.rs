//! Transport layer: WebSocket endpoint and per-connection loops.
//!
//! Transport stays dumb (architecture section 5.1): it parses envelopes,
//! authenticates once, then relays commands to the room task. All game logic
//! lives behind the room boundary.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::SinkExt;
use futures_util::stream::StreamExt;
use parcello_protocol::{AuthPayload, ClientMessage, ServerMessage};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use crate::AppState;
use crate::room::{ClientTx, RoomCmd, create_room};

/// Inbound frame/message ceiling. Every client -> server message in this JSON
/// protocol is small (commands, a <=500-char feedback comment); the default
/// tungstenite limits are 16 MiB/64 MiB. Capping both means a malicious client
/// cannot force a large allocation before the message is even parsed and
/// validated (this is what bounds the post-game comment path in particular).
/// Server -> client snapshots may exceed this; the cap is a *read* limit, and
/// tungstenite splits larger outbound messages into frames, so it never
/// truncates what the server sends.
const MAX_WS_MESSAGE_BYTES: usize = 64 * 1024;

/// Per-connection message-rate cap (token bucket): burst of `MSG_BURST`
/// messages, refilled at `MSG_REFILL_PER_SEC`. A client that sustains more
/// than this is flooding and gets closed. Generous vs. real play - moves are
/// seconds apart and even animation acks are bounded - so a legitimate client
/// never trips it; this only stops abuse.
const MSG_BURST: f64 = 32.0;
const MSG_REFILL_PER_SEC: f64 = 16.0;

/// Token-bucket message-rate limiter for one connection. Pure and clock-free
/// (the caller passes `now`), so its budget/refill behavior is unit-tested
/// without sockets or real time.
struct RateLimiter {
    tokens: f64,
    last: std::time::Instant,
}

impl RateLimiter {
    const fn new(now: std::time::Instant) -> Self {
        Self {
            tokens: MSG_BURST,
            last: now,
        }
    }

    /// Refill by elapsed time, then try to spend one token. Returns `false`
    /// when the connection is over budget and should be closed.
    fn allow(&mut self, now: std::time::Instant) -> bool {
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = elapsed
            .mul_add(MSG_REFILL_PER_SEC, self.tokens)
            .min(MSG_BURST);
        self.last = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(app): State<AppState>) -> Response {
    ws.max_frame_size(MAX_WS_MESSAGE_BYTES)
        .max_message_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, app))
}

/// Sender half plus the identity bound to this connection.
struct Session {
    room: mpsc::Sender<RoomCmd>,
    player_id: String,
    /// Watching, not playing (ADR-0035): every room-scoped message except
    /// leaving is refused at this boundary.
    spectator: bool,
}

async fn handle_socket(socket: WebSocket, app: AppState) {
    // Global connection cap: refuse (and immediately close) once the server is
    // saturated, so a flood of sockets cannot exhaust memory or descriptors.
    // The permit is held for the whole connection and released on return.
    let Ok(_permit) = app.connections.clone().try_acquire_owned() else {
        warn!(
            cap = crate::MAX_CONNECTIONS,
            "connection cap reached; refusing socket"
        );
        return;
    };

    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::unbounded_channel::<ServerMessage>();
    let writer = spawn_writer(sink, rx);

    let mut session: Option<Session> = None;
    // Ranked-queue membership for this connection (ADR-0034): the entry is
    // dropped on cancel, on entering a room, and when the socket closes.
    let mut queued: Option<String> = None;
    let mut limiter = RateLimiter::new(std::time::Instant::now());

    while let Some(frame) = stream.next().await {
        // A client that outruns the refill is flooding and gets disconnected.
        if !limiter.allow(std::time::Instant::now()) {
            warn!("message rate limit exceeded; closing connection");
            break;
        }

        let text = match frame {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue, // Binary/ping/pong frames are ignored or auto-handled.
        };
        let msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(msg) => msg,
            Err(e) => {
                send_error(&tx, &format!("malformed message: {e}"));
                continue;
            }
        };

        match (msg, &session) {
            (ClientMessage::Ping, _) => send(&tx, ServerMessage::Pong),

            // Mod discovery for the client's create-room picker (ADR-0006).
            // Connection-scoped like Ping: the answer feeds room *creation*,
            // so it must work before any room exists.
            (ClientMessage::ListMods, _) => send_mods(&app, &tx).await,

            // Ranked queue entry/exit and the ladder query (ADR-0034):
            // connection-scoped like ListMods. Queueing from inside a room
            // is refused; entering a room drops the queue entry below.
            (ClientMessage::QueueRanked { auth }, None) => {
                // A re-queue first replaces this connection's previous
                // entry, whatever identity it carried: a failed re-auth (or
                // an identity switch) must never leave an orphan in the
                // pool that the matchmaker would seat in a table this
                // client no longer expects.
                cancel_queue(&app, &mut queued, &tx);
                queued = handle_queue_ranked(&app, &auth, &tx).await;
            }
            (ClientMessage::QueueRanked { .. }, Some(_)) => {
                send_error(&tx, "leave the room before queueing ranked");
            }
            (ClientMessage::CancelQueue, _) => cancel_queue(&app, &mut queued, &tx),
            (ClientMessage::GetRating { auth }, _) => {
                handle_get_rating(&app, &auth, &tx).await;
            }

            (ClientMessage::Create { auth, mods }, None) => {
                session = handle_create(&app, auth, mods, &tx).await;
                drop_queue_entry_on_seat(&app, &mut queued, session.is_some(), &tx);
            }

            (ClientMessage::Join { code, auth }, None) => {
                session = handle_join(&app, &code, auth, &tx).await;
                drop_queue_entry_on_seat(&app, &mut queued, session.is_some(), &tx);
            }

            // Watch without a seat (ADR-0035); ends any ranked wait too.
            (ClientMessage::Spectate { code, auth }, None) => {
                session = handle_spectate(&app, code.as_deref(), &auth, &tx).await;
                drop_queue_entry_on_seat(&app, &mut queued, session.is_some(), &tx);
            }

            (
                ClientMessage::Create { .. }
                | ClientMessage::Join { .. }
                | ClientMessage::Spectate { .. },
                Some(_),
            ) => {
                send_error(&tx, "already in a room");
            }

            // Leaving keeps the socket open: the session is cleared so a
            // new Create/Join/Spectate can follow on the same connection.
            (ClientMessage::Leave, Some(s)) => {
                leave_room(s).await;
                session = None;
            }

            // Roomless no-ops: Leave with nothing to leave, and a stray
            // animation ack (harmless - acks release timers, never gate).
            (ClientMessage::Leave | ClientMessage::AnimationDone { .. }, None) => {}

            // Everything else is a room-scoped request: relay it verbatim -
            // unless this connection only watches (ADR-0035). Spectator
            // render acks are dropped silently (they gate nothing, and a
            // client naturally acks every Update it draws); anything else
            // from a spectator is refused.
            (msg, Some(s)) => {
                if s.spectator {
                    if !matches!(msg, ClientMessage::AnimationDone { .. }) {
                        send_error(&tx, "spectators can only watch; leave to stop");
                    }
                } else if s.room.send(relay(msg, &s.player_id)).await.is_err() {
                    break;
                }
            }

            (_, None) => send_error(&tx, "join a room first"),
        }
    }

    if let Some(s) = session {
        leave_room(&s).await;
    }
    // A closed socket leaves the queue (the matchmaker also prunes dead
    // senders each tick; this just makes it immediate).
    if let (Some(service), Some(player_id)) = (&app.ranked, queued) {
        service.remove(&player_id, &tx);
    }
    drop(tx); // Closes the writer task's channel.
    let _ = writer.await;
    debug!("connection closed");
}

/// Writer task: serialize outbound messages until the channel closes.
fn spawn_writer(
    mut sink: futures_util::stream::SplitSink<WebSocket, Message>,
    mut rx: mpsc::UnboundedReceiver<ServerMessage>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(json) => json,
                Err(e) => {
                    warn!(error = %e, "failed to serialize server message");
                    continue;
                }
            };
            if sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    })
}

/// Wraps a room-scoped client message into the `RoomCmd` that carries this
/// connection's bound identity. Exhaustive on purpose: adding a
/// `ClientMessage` variant must fail compilation here until it is either
/// relayed or routed in `handle_socket`'s connection-scoped arms.
fn relay(msg: ClientMessage, player_id: &str) -> RoomCmd {
    let player_id = player_id.to_owned();
    match msg {
        ClientMessage::Start => RoomCmd::Start { player_id },
        ClientMessage::PlayAgain => RoomCmd::PlayAgain { player_id },
        ClientMessage::AddBot => RoomCmd::AddBot { player_id },
        ClientMessage::RemoveBot => RoomCmd::RemoveBot { player_id },
        ClientMessage::Configure { settings } => RoomCmd::Configure {
            player_id,
            settings,
        },
        ClientMessage::Cmd { cmd } => RoomCmd::Game { player_id, cmd },
        ClientMessage::Feedback { rating, comment } => RoomCmd::Feedback {
            player_id,
            rating,
            comment,
        },
        ClientMessage::AnimationDone { through_seq } => RoomCmd::AnimationDone {
            player_id,
            through_seq,
        },
        // Connection-scoped messages are consumed by `handle_socket`'s
        // earlier arms and can never reach the relay.
        ClientMessage::Ping
        | ClientMessage::ListMods
        | ClientMessage::QueueRanked { .. }
        | ClientMessage::CancelQueue
        | ClientMessage::GetRating { .. }
        | ClientMessage::Create { .. }
        | ClientMessage::Join { .. }
        | ClientMessage::Spectate { .. }
        | ClientMessage::Leave => unreachable!("connection-scoped message reached relay"),
    }
}

/// Create flow: authenticate, resolve the room's mod set, register the room,
/// then join the creator to seat 0.
async fn handle_create(
    app: &AppState,
    auth: AuthPayload,
    mods: Option<Vec<String>>,
    tx: &ClientTx,
) -> Option<Session> {
    let identity = authenticate(app, &auth, tx)?;
    let content = match resolve_room_mods(app, mods).await {
        Ok(content) => content,
        Err(message) => {
            send(tx, ServerMessage::Error { message });
            return None;
        }
    };
    let code = match create_room(
        &app.rooms,
        content,
        app.history.clone(),
        app.turn_timeout,
        app.time_bank,
        app.game_timeout,
    )
    .await
    {
        Ok(code) => code,
        Err(message) => {
            send(tx, ServerMessage::Error { message });
            return None;
        }
    };
    send(tx, ServerMessage::RoomCreated { code: code.clone() });
    let room = app
        .rooms
        .read()
        .await
        .get(&code)
        .cloned()
        .expect("room registered by create_room");
    try_join(room, identity, auth.reconnect, tx).await
}

/// Join flow: authenticate, look the room up by code, take a seat.
async fn handle_join(
    app: &AppState,
    code: &str,
    auth: AuthPayload,
    tx: &ClientTx,
) -> Option<Session> {
    let identity = authenticate(app, &auth, tx)?;
    let room = app.rooms.read().await.get(&code.to_uppercase()).cloned();
    let Some(room) = room else {
        send(
            tx,
            ServerMessage::Error {
                message: format!("no room with code {code}"),
            },
        );
        return None;
    };
    try_join(room, identity, auth.reconnect, tx).await
}

/// Spectate flow (ADR-0035): authenticate like a join, resolve which room
/// to watch (an explicit code, or the server's pick), and attach as a
/// seatless watcher.
async fn handle_spectate(
    app: &AppState,
    code: Option<&str>,
    auth: &AuthPayload,
    tx: &ClientTx,
) -> Option<Session> {
    let identity = authenticate(app, auth, tx)?;
    let room = match code {
        Some(code) => app.rooms.read().await.get(&code.to_uppercase()).cloned(),
        None => pick_watchable_room(app).await,
    };
    let Some(room) = room else {
        let message = match code {
            Some(code) => format!("no room with code {code}"),
            None => "nothing to watch right now".into(),
        };
        send(tx, ServerMessage::Error { message });
        return None;
    };
    let (reply, on_reply) = oneshot::channel();
    let join = RoomCmd::SpectateJoin {
        identity,
        tx: tx.clone(),
        reply,
    };
    if room.send(join).await.is_err() {
        send_error(tx, "room no longer exists");
        return None;
    }
    match on_reply.await {
        // The session carries the watcher's unique routing key, not the
        // bare identity: its Disconnect must never shadow a seat's.
        Ok(Ok(watcher_key)) => Some(Session {
            room,
            player_id: watcher_key,
            spectator: true,
        }),
        Ok(Err(message)) => {
            send(tx, ServerMessage::Error { message });
            None
        }
        Err(_) => {
            send_error(tx, "room closed during spectate");
            None
        }
    }
}

/// The server's pick for "watch anything" (ADR-0035): the Active room with
/// the most connected humans - which degrades naturally to the bots
/// showcase (zero humans) when it is the only game running.
async fn pick_watchable_room(app: &AppState) -> Option<mpsc::Sender<RoomCmd>> {
    let handles: Vec<mpsc::Sender<RoomCmd>> = app.rooms.read().await.values().cloned().collect();
    let mut best: Option<(usize, mpsc::Sender<RoomCmd>)> = None;
    for handle in handles {
        let (reply, on_reply) = oneshot::channel();
        if handle.send(RoomCmd::Probe { reply }).await.is_err() {
            continue; // Room dissolved between listing and probing.
        }
        let Ok(probe) = on_reply.await else { continue };
        if probe.active && best.as_ref().is_none_or(|(h, _)| probe.humans > *h) {
            best = Some((probe.humans, handle));
        }
    }
    best.map(|(_, handle)| handle)
}

/// Per-room mod set (ADR-0006): `None` or `[]` selects the server default.
/// Mod ids come from the wire and end up in filesystem paths, so they are
/// allowlist-validated here before touching `mods_dir`.
async fn resolve_room_mods(
    app: &AppState,
    mods: Option<Vec<String>>,
) -> Result<std::sync::Arc<parcello_mods::ResolvedContent>, String> {
    let ids = match mods {
        None => return Ok(app.content.clone()),
        Some(ids) if ids.is_empty() => return Ok(app.content.clone()),
        Some(ids) => ids,
    };
    if ids.len() > 16 {
        return Err("too many mods (max 16)".into());
    }
    if let Some(bad) = ids.iter().find(|id| !valid_mod_id(id)) {
        return Err(format!("invalid mod id: {bad}"));
    }
    let dir = std::sync::Arc::clone(&app.mods_dir);
    // Small local TOML reads, but still filesystem I/O: keep it off the
    // async executor threads.
    tokio::task::spawn_blocking(move || parcello_mods::resolve(&dir, &ids))
        .await
        .map_err(|_| "mod resolution task failed".to_string())?
        .map(std::sync::Arc::new)
        .map_err(|e| format!("mod resolution failed: {e}"))
}

/// The mod ids this server can resolve: the subdirectories of `mods_dir`,
/// filtered through the same `valid_mod_id` allowlist a Create is held to
/// (never advertise an id we would then refuse), sorted for a stable wire
/// shape. Read from disk per request rather than cached at boot on purpose:
/// room creation also resolves from disk, so a mod dropped in while the
/// server runs is already creatable - the picker must see it too. A readdir
/// is still filesystem I/O, so it stays off the async executor threads.
async fn list_mods(app: &AppState) -> Vec<String> {
    let dir = std::sync::Arc::clone(&app.mods_dir);
    tokio::task::spawn_blocking(move || {
        let mut ids: Vec<String> = std::fs::read_dir(&*dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|id| valid_mod_id(id))
            .collect();
        ids.sort();
        ids
    })
    .await
    .unwrap_or_default()
}

/// Tells the room this connection is gone (seat kept for rejoin, spectator
/// entry dropped); shared by `Leave` and the socket-close path.
async fn leave_room(s: &Session) {
    let _ = s
        .room
        .send(RoomCmd::Disconnect {
            player_id: s.player_id.clone(),
        })
        .await;
}

/// Reply to `ListMods` (ADR-0006): the picker-ready id list.
async fn send_mods(app: &AppState, tx: &ClientTx) {
    send(
        tx,
        ServerMessage::Mods {
            ids: list_mods(app).await,
        },
    );
}

/// Directory-name charset only: no separators, no dots, so a wire-supplied
/// id can never escape `mods_dir`.
fn valid_mod_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Queue entry (ADR-0034): authenticate, refuse spoofable identities (a
/// persistent rating needs an unforgeable id), read the caller's rating off
/// the executor threads, enqueue. Returns the queued `player_id` so the
/// connection can clean its entry up later.
async fn handle_queue_ranked(
    app: &AppState,
    auth: &parcello_protocol::AuthPayload,
    tx: &ClientTx,
) -> Option<String> {
    let Some(service) = &app.ranked else {
        send(
            tx,
            ServerMessage::Error {
                message: "ranked matchmaking is disabled on this server".into(),
            },
        );
        return None;
    };
    let identity = authenticate(app, auth, tx)?;
    if identity.spoofable {
        send(
            tx,
            ServerMessage::Error {
                message:
                    "ranked requires a signed-in account; guest identities cannot hold a rating"
                        .into(),
            },
        );
        return None;
    }
    let player_id = identity.player_id.clone();
    let mu = {
        let store = std::sync::Arc::clone(&service.store);
        let id = player_id.clone();
        // Rating reads may hit SQLite: keep them off the async executor.
        tokio::task::spawn_blocking(move || store.get(&id).rating.mu)
            .await
            .unwrap_or_else(|_| crate::ranked::ladder::Rating::default().mu)
    };
    service.enqueue(identity, mu, tx.clone());
    Some(player_id)
}

/// Ladder record query (ADR-0034), feeding the client's menu player card.
async fn handle_get_rating(app: &AppState, auth: &parcello_protocol::AuthPayload, tx: &ClientTx) {
    let Some(service) = &app.ranked else {
        send(
            tx,
            ServerMessage::Error {
                message: "ranked matchmaking is disabled on this server".into(),
            },
        );
        return;
    };
    let Some(identity) = authenticate(app, auth, tx) else {
        return;
    };
    if identity.spoofable {
        send(
            tx,
            ServerMessage::Error {
                message: "guest identities have no rating; sign in first".into(),
            },
        );
        return;
    }
    let store = std::sync::Arc::clone(&service.store);
    let player_id = identity.player_id;
    let record = {
        let id = player_id.clone();
        tokio::task::spawn_blocking(move || store.get(&id)).await
    };
    let Ok(record) = record else {
        send(
            tx,
            ServerMessage::Error {
                message: "rating lookup failed".into(),
            },
        );
        return;
    };
    send(
        tx,
        ServerMessage::Rating {
            player_id,
            mu: record.rating.mu,
            sigma: record.rating.sigma,
            games: record.games,
            wins: record.wins,
            display: crate::ranked::ladder::display(record.rating),
        },
    );
}

/// Taking a seat anywhere ends the wait (ADR-0034): a player cannot sit at
/// a table and stay in the ranked queue at once.
fn drop_queue_entry_on_seat(
    app: &AppState,
    queued: &mut Option<String>,
    seated: bool,
    tx: &ClientTx,
) {
    if seated && let (Some(service), Some(player_id)) = (&app.ranked, queued.take()) {
        service.remove(&player_id, tx);
    }
}

/// Auth happens once per connection, at Create/Join time. The room binds the
/// resulting identity to the connection's sender; the wire identity is never
/// trusted again afterwards.
fn authenticate(
    app: &AppState,
    auth: &AuthPayload,
    tx: &ClientTx,
) -> Option<crate::auth::Identity> {
    match app.verifier.verify(auth) {
        Ok(identity) => Some(identity),
        Err(message) => {
            send(tx, ServerMessage::Error { message });
            None
        }
    }
}

async fn try_join(
    room: mpsc::Sender<RoomCmd>,
    identity: crate::auth::Identity,
    reconnect: Option<String>,
    tx: &ClientTx,
) -> Option<Session> {
    let player_id = identity.player_id.clone();
    let (reply, on_reply) = oneshot::channel();
    let join = RoomCmd::Join {
        identity,
        reconnect,
        tx: tx.clone(),
        reply,
    };
    if room.send(join).await.is_err() {
        send(
            tx,
            ServerMessage::Error {
                message: "room no longer exists".into(),
            },
        );
        return None;
    }
    match on_reply.await {
        Ok(Ok(())) => Some(Session {
            room,
            player_id,
            spectator: false,
        }),
        Ok(Err(message)) => {
            send(tx, ServerMessage::Error { message });
            None
        }
        Err(_) => {
            send(
                tx,
                ServerMessage::Error {
                    message: "room closed during join".into(),
                },
            );
            None
        }
    }
}

fn send(tx: &ClientTx, msg: ServerMessage) {
    let _ = tx.send(msg);
}

fn send_error(tx: &ClientTx, message: &str) {
    send(
        tx,
        ServerMessage::Error {
            message: message.to_string(),
        },
    );
}

/// Leaves the ranked queue (ADR-0034) and confirms the new pool size.
fn cancel_queue(app: &AppState, queued: &mut Option<String>, tx: &ClientTx) {
    if let (Some(service), Some(player_id)) = (&app.ranked, queued.take()) {
        service.remove(&player_id, tx);
        send(
            tx,
            ServerMessage::Queued {
                size: service.len(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{MSG_BURST, MSG_REFILL_PER_SEC, RateLimiter, valid_mod_id};
    use std::time::{Duration, Instant};

    #[test]
    fn rate_limiter_allows_a_burst_then_blocks_then_refills() {
        let t0 = Instant::now();
        let mut rl = RateLimiter::new(t0);
        // The whole burst passes with no time elapsed...
        for _ in 0..MSG_BURST as usize {
            assert!(rl.allow(t0));
        }
        // ...the very next frame (still t0) is over budget.
        assert!(!rl.allow(t0));
        // One second later exactly MSG_REFILL_PER_SEC frames are allowed again.
        let t1 = t0 + Duration::from_secs(1);
        let allowed = (0..100).filter(|_| rl.allow(t1)).count();
        assert_eq!(allowed, MSG_REFILL_PER_SEC as usize);
    }

    #[test]
    fn mod_ids_cannot_escape_the_mods_dir() {
        for ok in ["base", "my-mod_2", "A"] {
            assert!(valid_mod_id(ok), "{ok} should be accepted");
        }
        for bad in [
            "",
            "..",
            "../etc",
            "a/b",
            "a\\b",
            "mod.toml",
            "C:evil",
            &"x".repeat(65),
        ] {
            assert!(!valid_mod_id(bad), "{bad:?} should be rejected");
        }
    }
}
