//! Transport layer: WebSocket endpoint and per-connection loops.
//!
//! Transport stays dumb (architecture section 5.1): it parses envelopes,
//! authenticates once, then relays commands to the room task. All game logic
//! lives behind the room boundary.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use parcello_protocol::{AuthPayload, ClientMessage, ServerMessage};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use crate::room::{create_room, ClientTx, RoomCmd};
use crate::AppState;

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
            if sink.send(Message::Text(json)).await.is_err() {
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

        match (msg, &session) {
            (ClientMessage::Ping, _) => send(&tx, ServerMessage::Pong),

            (ClientMessage::Create { auth }, None) => {
                let Some(identity) = authenticate(&app, &auth, &tx) else {
                    continue;
                };
                let code = match create_room(
                    &app.rooms,
                    app.content.clone(),
                    app.history.clone(),
                    app.turn_timeout,
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
                session = try_join(room, identity, &tx).await;
            }

            (ClientMessage::Join { code, auth }, None) => {
                let Some(identity) = authenticate(&app, &auth, &tx) else {
                    continue;
                };
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
                session = try_join(room, identity, &tx).await;
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

            (ClientMessage::Cmd { cmd }, Some(s)) => {
                let cmd = RoomCmd::Game {
                    player_id: s.player_id.clone(),
                    cmd,
                };
                if s.room.send(cmd).await.is_err() {
                    break;
                }
            }

            (ClientMessage::Start | ClientMessage::Cmd { .. }, None) => {
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
    tx: &ClientTx,
) -> Option<Session> {
    let player_id = identity.player_id.clone();
    let (reply, on_reply) = oneshot::channel();
    let join = RoomCmd::Join {
        identity,
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
