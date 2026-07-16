//! Transport integration tests: a real axum server on an ephemeral port,
//! spoken to over genuine `WebSockets`. Exercises the connection state
//! machine in `ws.rs` (authenticate-once, relay, leave-then-rejoin) that
//! unit tests cannot reach.

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use parcello_server::auth::CompositeVerifier;
use parcello_server::history::MemoryHistory;
use parcello_server::room::Rooms;
use parcello_server::{AppState, game_router};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Boots the real router on an ephemeral port with guest auth and the
/// bundled base mod, returning the ws:// URL.
async fn spawn_server() -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mods_dir = manifest
        .join("../../mods")
        .canonicalize()
        .expect("mods dir");
    let resolved = parcello_mods::resolve(&mods_dir, &["base".to_string()]).expect("base resolves");
    let state = AppState {
        rooms: Rooms::default(),
        content: Arc::new(resolved),
        mods_dir: Arc::new(mods_dir),
        verifier: Arc::new(CompositeVerifier::new(None, None, true)),
        history: Arc::new(MemoryHistory::new()),
        turn_timeout: None,
        time_bank: None,
        game_timeout: None,
        connections: AppState::connection_limiter(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, game_router(state)).await.unwrap();
    });
    format!("ws://{addr}/ws")
}

async fn connect(url: &str) -> Socket {
    let (socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connect");
    socket
}

async fn send(socket: &mut Socket, msg: Value) {
    socket
        .send(Message::Text(msg.to_string().into()))
        .await
        .expect("send");
}

/// Next JSON frame, with a hard timeout so a hung test fails fast.
async fn recv(socket: &mut Socket) -> Value {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
        .await
        .expect("server reply within 5s")
        .expect("stream open")
        .expect("frame ok");
    match frame {
        Message::Text(text) => serde_json::from_str(&text).expect("valid json"),
        other => panic!("expected a text frame, got {other:?}"),
    }
}

/// Skips broadcast frames until one of type `wanted` arrives.
async fn recv_until(socket: &mut Socket, wanted: &str) -> Value {
    for _ in 0..32 {
        let msg = recv(socket).await;
        if msg["type"] == wanted {
            return msg;
        }
    }
    panic!("no `{wanted}` frame within 32 messages");
}

fn guest(name: &str) -> Value {
    json!({ "guest_name": name })
}

#[tokio::test]
async fn create_join_start_and_relay_flow() {
    let url = spawn_server().await;

    // Host creates; gets the code, then a Joined with its seat.
    let mut host = connect(&url).await;
    send(&mut host, json!({"type": "create", "auth": guest("alice")})).await;
    let created = recv(&mut host).await;
    assert_eq!(created["type"], "room_created");
    let code = created["code"].as_str().expect("code").to_string();
    let joined = recv_until(&mut host, "joined").await;
    assert_eq!(joined["seat"], 0, "creator takes seat 0 (host)");
    assert!(
        joined["reconnect"].is_string(),
        "guest seats get a reconnect token (ADR-0008)"
    );

    // Guest joins by code (case-insensitive on the wire).
    let mut guest2 = connect(&url).await;
    send(
        &mut guest2,
        json!({"type": "join", "code": code.to_lowercase(), "auth": guest("bob")}),
    )
    .await;
    let joined2 = recv_until(&mut guest2, "joined").await;
    assert_eq!(joined2["seat"], 1);

    // Host starts: both ends must see GameStarted (the relay path works).
    send(&mut host, json!({"type": "start"})).await;
    let started_host = recv_until(&mut host, "game_started").await;
    let started_guest = recv_until(&mut guest2, "game_started").await;
    assert_eq!(started_host["view"]["players"].as_array().unwrap().len(), 2);
    assert_eq!(
        started_host["view"]["current"], started_guest["view"]["current"],
        "both clients see the same acting seat"
    );
}

#[tokio::test]
async fn room_scoped_messages_require_a_room_and_create_requires_leaving() {
    let url = spawn_server().await;
    let mut socket = connect(&url).await;

    // Ping works roomless.
    send(&mut socket, json!({"type": "ping"})).await;
    assert_eq!(recv(&mut socket).await["type"], "pong");

    // A game command without a room is an explicit error.
    send(
        &mut socket,
        json!({"type": "cmd", "cmd": {"type": "end_turn"}}),
    )
    .await;
    let err = recv(&mut socket).await;
    assert_eq!(err["type"], "error");
    assert!(err["message"].as_str().unwrap().contains("join a room"));

    // Malformed JSON is rejected without killing the connection.
    socket
        .send(Message::Text("{not json".to_string().into()))
        .await
        .expect("send");
    let err = recv(&mut socket).await;
    assert_eq!(err["type"], "error");
    assert!(err["message"].as_str().unwrap().contains("malformed"));

    // Create, then a second create on the same connection is refused...
    send(
        &mut socket,
        json!({"type": "create", "auth": guest("solo")}),
    )
    .await;
    recv_until(&mut socket, "joined").await;
    send(
        &mut socket,
        json!({"type": "create", "auth": guest("solo")}),
    )
    .await;
    // Lobby broadcasts may still be in flight; wait for the error frame.
    let err = recv_until(&mut socket, "error").await;
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("already in a room")
    );

    // ...until Leave clears the session; then the same socket can create
    // again (the Flutter connect/menu split relies on this).
    send(&mut socket, json!({"type": "leave"})).await;
    send(
        &mut socket,
        json!({"type": "create", "auth": guest("solo")}),
    )
    .await;
    assert_eq!(
        recv_until(&mut socket, "room_created").await["type"],
        "room_created"
    );
}

#[tokio::test]
async fn joining_an_unknown_room_or_without_auth_fails_cleanly() {
    let url = spawn_server().await;

    let mut socket = connect(&url).await;
    send(
        &mut socket,
        json!({"type": "join", "code": "ZZZZZ", "auth": guest("bob")}),
    )
    .await;
    let err = recv(&mut socket).await;
    assert_eq!(err["type"], "error");
    assert!(err["message"].as_str().unwrap().contains("no room"));

    // Empty auth payload: the verifier rejects before any room logic runs.
    let mut anon = connect(&url).await;
    send(&mut anon, json!({"type": "create", "auth": {}})).await;
    assert_eq!(recv(&mut anon).await["type"], "error");
}

#[tokio::test]
async fn guest_seat_rejoin_requires_the_reconnect_token() {
    let url = spawn_server().await;

    let mut first = connect(&url).await;
    send(&mut first, json!({"type": "create", "auth": guest("ada")})).await;
    let code = recv(&mut first).await["code"].as_str().unwrap().to_string();
    let token = recv_until(&mut first, "joined").await["reconnect"]
        .as_str()
        .unwrap()
        .to_string();

    // Same guest name, no token: the seat is protected (ADR-0008).
    let mut thief = connect(&url).await;
    send(
        &mut thief,
        json!({"type": "join", "code": code, "auth": guest("ada")}),
    )
    .await;
    let err = recv(&mut thief).await;
    assert_eq!(err["type"], "error");
    assert!(err["message"].as_str().unwrap().contains("reconnect token"));

    // With the token: rejoin succeeds and keeps the same seat.
    let mut back = connect(&url).await;
    send(
        &mut back,
        json!({"type": "join", "code": code,
               "auth": {"guest_name": "ada", "reconnect": token}}),
    )
    .await;
    assert_eq!(recv_until(&mut back, "joined").await["seat"], 0);
}

#[tokio::test]
async fn list_mods_answers_before_any_room_exists() {
    let url = spawn_server().await;

    // Connection-scoped like ping: no create/join first.
    let mut client = connect(&url).await;
    send(&mut client, json!({"type": "list_mods"})).await;
    let reply = recv(&mut client).await;
    assert_eq!(reply["type"], "mods");
    let ids: Vec<&str> = reply["ids"]
        .as_array()
        .expect("ids array")
        .iter()
        .map(|v| v.as_str().expect("string id"))
        .collect();
    // The repo mods dir ships `base` (+ `highroller`); sorted wire shape.
    assert!(ids.contains(&"base"), "bundled base mod must be listed");
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "ids arrive sorted");
}
