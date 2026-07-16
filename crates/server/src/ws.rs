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

pub async fn ws_handler(ws: WebSocketUpgrade, State(app): State<AppState>) -> Response {
    ws.max_frame_size(MAX_WS_MESSAGE_BYTES)
        .max_message_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, app))
}

/// Sender half plus the identity bound to this connection.
struct Session {
    room: mpsc::Sender<RoomCmd>,
    player_id: String,
}

async fn handle_socket(socket: WebSocket, app: AppState) {
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::unbounded_channel::<ServerMessage>();
    let writer = spawn_writer(sink, rx);

    let mut session: Option<Session> = None;

    while let Some(frame) = stream.next().await {
        let text = match frame {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue, // Binary/ping/pong frames are ignored or auto-handled.
        };
        let msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(msg) => msg,
            Err(e) => {
                send(
                    &tx,
                    ServerMessage::Error {
                        message: format!("malformed message: {e}"),
                    },
                );
                continue;
            }
        };

        match (msg, &session) {
            (ClientMessage::Ping, _) => send(&tx, ServerMessage::Pong),

            (ClientMessage::Create { auth, mods }, None) => {
                session = handle_create(&app, auth, mods, &tx).await;
            }

            (ClientMessage::Join { code, auth }, None) => {
                session = handle_join(&app, &code, auth, &tx).await;
            }

            (ClientMessage::Create { .. } | ClientMessage::Join { .. }, Some(_)) => {
                send(
                    &tx,
                    ServerMessage::Error {
                        message: "already in a room".into(),
                    },
                );
            }

            // Leaving keeps the socket open: the session is cleared so a
            // new Create/Join can follow on the same connection.
            (ClientMessage::Leave, Some(s)) => {
                let _ = s
                    .room
                    .send(RoomCmd::Disconnect {
                        player_id: s.player_id.clone(),
                    })
                    .await;
                session = None;
            }

            // Roomless no-ops: Leave with nothing to leave, and a stray
            // animation ack (harmless - acks release timers, never gate).
            (ClientMessage::Leave | ClientMessage::AnimationDone { .. }, None) => {}

            // Everything else is a room-scoped request: relay it verbatim.
            (msg, Some(s)) => {
                if s.room.send(relay(msg, &s.player_id)).await.is_err() {
                    break;
                }
            }

            (_, None) => {
                send(
                    &tx,
                    ServerMessage::Error {
                        message: "join a room first".into(),
                    },
                );
            }
        }
    }

    if let Some(s) = session {
        let _ = s
            .room
            .send(RoomCmd::Disconnect {
                player_id: s.player_id,
            })
            .await;
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
        | ClientMessage::Create { .. }
        | ClientMessage::Join { .. }
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

/// Directory-name charset only: no separators, no dots, so a wire-supplied
/// id can never escape `mods_dir`.
fn valid_mod_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
        Ok(Ok(())) => Some(Session { room, player_id }),
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

#[cfg(test)]
mod tests {
    use super::valid_mod_id;

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
