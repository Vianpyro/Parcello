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

pub async fn ws_handler(ws: WebSocketUpgrade, State(app): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, app))
}

/// Sender half plus the identity bound to this connection.
struct Session {
    room: mpsc::Sender<RoomCmd>,
    player_id: String,
}

async fn handle_socket(socket: WebSocket, app: AppState) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Writer task: serialize outbound messages until the channel closes.
    let writer = tokio::spawn(async move {
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
    });

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

        // Set when the client leaves its room but keeps the socket open; the
        // session is cleared after the match so a new Create/Join can follow.
        let mut leave = false;
        match (msg, &session) {
            (ClientMessage::Ping, _) => send(&tx, ServerMessage::Pong),

            (ClientMessage::Create { auth, mods }, None) => {
                let Some(identity) = authenticate(&app, &auth, &tx) else {
                    continue;
                };
                let reconnect = auth.reconnect;
                let content = match resolve_room_mods(&app, mods).await {
                    Ok(content) => content,
                    Err(message) => {
                        send(&tx, ServerMessage::Error { message });
                        continue;
                    }
                };
                let code = match create_room(
                    &app.rooms,
                    content,
                    app.history.clone(),
                    app.turn_timeout,
                    app.game_timeout,
                )
                .await
                {
                    Ok(code) => code,
                    Err(message) => {
                        send(&tx, ServerMessage::Error { message });
                        continue;
                    }
                };
                send(&tx, ServerMessage::RoomCreated { code: code.clone() });
                let room = app
                    .rooms
                    .read()
                    .await
                    .get(&code)
                    .cloned()
                    .expect("room registered by create_room");
                session = try_join(room, identity, reconnect, &tx).await;
            }

            (ClientMessage::Join { code, auth }, None) => {
                let Some(identity) = authenticate(&app, &auth, &tx) else {
                    continue;
                };
                let reconnect = auth.reconnect;
                let room = app.rooms.read().await.get(&code.to_uppercase()).cloned();
                let Some(room) = room else {
                    send(
                        &tx,
                        ServerMessage::Error {
                            message: format!("no room with code {code}"),
                        },
                    );
                    continue;
                };
                session = try_join(room, identity, reconnect, &tx).await;
            }

            (ClientMessage::Create { .. } | ClientMessage::Join { .. }, Some(_)) => {
                send(
                    &tx,
                    ServerMessage::Error {
                        message: "already in a room".into(),
                    },
                );
            }

            (ClientMessage::Start, Some(s)) => {
                let cmd = RoomCmd::Start {
                    player_id: s.player_id.clone(),
                };
                if s.room.send(cmd).await.is_err() {
                    break;
                }
            }

            (ClientMessage::PlayAgain, Some(s)) => {
                let cmd = RoomCmd::PlayAgain {
                    player_id: s.player_id.clone(),
                };
                if s.room.send(cmd).await.is_err() {
                    break;
                }
            }

            (ClientMessage::Leave, Some(s)) => {
                let _ = s
                    .room
                    .send(RoomCmd::Disconnect {
                        player_id: s.player_id.clone(),
                    })
                    .await;
                leave = true;
            }
            (ClientMessage::Leave, None) => {} // already roomless

            (ClientMessage::Cmd { cmd }, Some(s)) => {
                let cmd = RoomCmd::Game {
                    player_id: s.player_id.clone(),
                    cmd,
                };
                if s.room.send(cmd).await.is_err() {
                    break;
                }
            }

            (ClientMessage::Feedback { rating, comment }, Some(s)) => {
                let cmd = RoomCmd::Feedback {
                    player_id: s.player_id.clone(),
                    rating,
                    comment,
                };
                if s.room.send(cmd).await.is_err() {
                    break;
                }
            }

            (
                ClientMessage::Start
                | ClientMessage::PlayAgain
                | ClientMessage::Cmd { .. }
                | ClientMessage::Feedback { .. },
                None,
            ) => {
                send(
                    &tx,
                    ServerMessage::Error {
                        message: "join a room first".into(),
                    },
                );
            }
        }
        if leave {
            session = None;
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
