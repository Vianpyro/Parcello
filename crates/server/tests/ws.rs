//! Transport integration tests: a real axum server on an ephemeral port,
//! spoken to over genuine `WebSockets`. Exercises the connection state
//! machine in `ws.rs` (authenticate-once, relay, leave-then-rejoin) that
//! unit tests cannot reach.

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use parcello_server::auth::CompositeVerifier;
use parcello_server::history::MemoryHistory;
use parcello_server::ranked::{MemoryRatings, RankedConfig, RankedService, spawn_matchmaker};
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
        default_issuer: None,
        connections: AppState::connection_limiter(),
        ranked: None,
        guest_allowed: true,
        showcase: false,
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

/// Boots the router with ranked matchmaking enabled (ADR-0034): HS256 token
/// auth (so identities are non-spoofable), guests allowed (to test their
/// rejection), an in-memory rating store, and a fast matchmaker tuned for
/// two-seat tables so tests never wait on the real 60s fallback.
async fn spawn_ranked_server(secret: &str) -> String {
    spawn_ranked_server_with_grace(secret, std::time::Duration::from_secs(10)).await
}

async fn spawn_ranked_server_with_grace(secret: &str, start_grace: std::time::Duration) -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mods_dir = manifest
        .join("../../mods")
        .canonicalize()
        .expect("mods dir");
    let resolved = parcello_mods::resolve(&mods_dir, &["base".to_string()]).expect("base resolves");
    let ranked = RankedService::new(
        Arc::new(MemoryRatings::new()),
        RankedConfig {
            target_seats: 2,
            fallback: std::time::Duration::from_mins(1),
            start_grace,
            tick: std::time::Duration::from_millis(50),
        },
    );
    let state = AppState {
        rooms: Rooms::default(),
        content: Arc::new(resolved),
        mods_dir: Arc::new(mods_dir),
        verifier: Arc::new(CompositeVerifier::new(None, Some(secret.into()), true)),
        history: Arc::new(MemoryHistory::new()),
        turn_timeout: None,
        time_bank: None,
        game_timeout: None,
        default_issuer: None,
        connections: AppState::connection_limiter(),
        ranked: Some(ranked),
        guest_allowed: true,
        showcase: false,
    };
    spawn_matchmaker(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, game_router(state)).await.unwrap();
    });
    format!("ws://{addr}/ws")
}

/// A signed HS256 token for `sub` (the deprecated-but-supported stopgap,
/// ADR-0003): the cheapest non-spoofable identity a test can mint.
fn hs256_token(secret: &str, sub: &str) -> String {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use hmac::{Hmac, KeyInit, Mac};

    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload =
        URL_SAFE_NO_PAD.encode(format!(r#"{{"sub":"{sub}","name":"{sub}","exp":{exp}}}"#));
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{header}.{payload}").as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{header}.{payload}.{sig}")
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

#[tokio::test]
async fn config_json_advertises_default_issuer() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        default_issuer: Some("https://auth.example.com".to_string()),
        connections: AppState::connection_limiter(),
        ranked: None,
        // Deliberately false: proves the advertised value follows the
        // state, so clients can trust it to hide the guest path.
        guest_allowed: false,
        showcase: false,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, game_router(state)).await.unwrap();
    });

    // Raw HTTP/1.1 GET: no HTTP client is in dev-deps, and the endpoint is
    // small enough that a hand-rolled request is cheaper than adding one.
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    stream
        .write_all(b"GET /config.json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write request");
    let mut resp = String::new();
    stream
        .read_to_string(&mut resp)
        .await
        .expect("read response");

    assert!(resp.starts_with("HTTP/1.1 200"), "status line: {resp}");
    assert!(
        resp.contains(r#""default_issuer":"https://auth.example.com""#),
        "body should carry the configured issuer: {resp}"
    );
    assert!(
        resp.contains(r#""guest_allowed":false"#),
        "no --insecure-guest: the config must say so, so clients hide the \
         guest path: {resp}"
    );
}

#[tokio::test]
async fn ranked_flow_queues_matches_plays_and_rates() {
    let secret = "test-secret";
    let url = spawn_ranked_server(secret).await;

    // Two token-authenticated players enter the queue.
    let mut a = connect(&url).await;
    send(
        &mut a,
        json!({"type": "queue_ranked", "auth": {"token": hs256_token(secret, "ua")}}),
    )
    .await;
    assert_eq!(recv_until(&mut a, "queued").await["size"], 1);

    let mut b = connect(&url).await;
    send(
        &mut b,
        json!({"type": "queue_ranked", "auth": {"token": hs256_token(secret, "ub")}}),
    )
    .await;

    // The matchmaker forms a two-seat table and both get the room code.
    let found_a = recv_until(&mut a, "match_found").await;
    let found_b = recv_until(&mut b, "match_found").await;
    let code = found_a["code"].as_str().expect("code").to_string();
    assert_eq!(found_b["code"], code.as_str());

    // Both join with a normal Join; the room flags itself ranked...
    send(
        &mut a,
        json!({"type": "join", "code": code, "auth": {"token": hs256_token(secret, "ua")}}),
    )
    .await;
    let joined = recv_until(&mut a, "joined").await;
    assert_eq!(joined["ranked"], true, "ranked marker rides Joined");
    send(
        &mut b,
        json!({"type": "join", "code": code, "auth": {"token": hs256_token(secret, "ub")}}),
    )
    .await;

    // ...an outsider (even token-authenticated) is refused a seat...
    let mut outsider = connect(&url).await;
    send(
        &mut outsider,
        json!({"type": "join", "code": code, "auth": {"token": hs256_token(secret, "ux")}}),
    )
    .await;
    let err = recv_until(&mut outsider, "error").await;
    assert!(err["message"].as_str().unwrap().contains("ranked match"));

    // ...and the game auto-starts once every matched player arrived.
    recv_until(&mut a, "game_started").await;
    recv_until(&mut b, "game_started").await;

    // Ranked rooms have no host: host powers are rejected for everyone.
    send(&mut a, json!({"type": "add_bot"})).await;
    let err = recv_until(&mut a, "error").await;
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("not available in a ranked match")
    );

    // Seat a resigns (turn-exempt like trading): last player standing wins
    // and the result is rated - both ends see the rating movements.
    send(&mut a, json!({"type": "cmd", "cmd": {"type": "resign"}})).await;
    let updated = recv_until(&mut b, "ratings_updated").await;
    let changes = updated["changes"].as_array().expect("changes");
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["player_id"], "hs256:ub", "winner listed first");
    assert!(
        changes[0]["display_delta"].as_i64().unwrap() > 0,
        "winner climbs"
    );
    assert_eq!(changes[1]["player_id"], "hs256:ua");
    assert!(
        changes[1]["display_delta"].as_i64().unwrap() <= 0,
        "resigner does not climb"
    );
    recv_until(&mut a, "ratings_updated").await;

    // Replay goes back through the queue: PlayAgain is a host power and
    // ranked rooms have none.
    send(&mut b, json!({"type": "play_again"})).await;
    let err = recv_until(&mut b, "error").await;
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("not available in a ranked match")
    );

    // The ladder remembers: the winner's record now shows 1 win / 1 game.
    let mut c = connect(&url).await;
    send(
        &mut c,
        json!({"type": "get_rating", "auth": {"token": hs256_token(secret, "ub")}}),
    )
    .await;
    let rating = recv_until(&mut c, "rating").await;
    assert_eq!(rating["player_id"], "hs256:ub");
    assert_eq!(rating["games"], 1);
    assert_eq!(rating["wins"], 1);
    assert!(rating["display"].as_i64().unwrap() > 1000);
}

#[tokio::test]
async fn ranked_queue_rejects_guests_and_requeue_leaves_no_orphan() {
    let secret = "test-secret";
    let url = spawn_ranked_server(secret).await;

    // Spoofable identities cannot hold a rating (ADR-0034).
    let mut g = connect(&url).await;
    send(
        &mut g,
        json!({"type": "queue_ranked", "auth": guest("mallory")}),
    )
    .await;
    let err = recv_until(&mut g, "error").await;
    assert!(err["message"].as_str().unwrap().contains("signed-in"));

    // A fresh token identity reads the default ladder record.
    send(
        &mut g,
        json!({"type": "get_rating", "auth": {"token": hs256_token(secret, "fresh")}}),
    )
    .await;
    let rating = recv_until(&mut g, "rating").await;
    assert_eq!(rating["display"], 1000);
    assert_eq!(rating["games"], 0);

    // A queued player whose re-queue fails auth leaves no orphan entry:
    // the pool must be empty again, so a second player queueing alone
    // sees size 1 (an orphan would read 2 and could burn a match on a
    // connection that no longer expects one).
    let mut a = connect(&url).await;
    send(
        &mut a,
        json!({"type": "queue_ranked", "auth": {"token": hs256_token(secret, "ua")}}),
    )
    .await;
    assert_eq!(recv_until(&mut a, "queued").await["size"], 1);
    send(&mut a, json!({"type": "queue_ranked", "auth": guest("ua")})).await;
    let err = recv_until(&mut a, "error").await;
    assert!(err["message"].as_str().unwrap().contains("signed-in"));

    let mut b = connect(&url).await;
    send(
        &mut b,
        json!({"type": "queue_ranked", "auth": {"token": hs256_token(secret, "ub")}}),
    )
    .await;
    assert_eq!(
        recv_until(&mut b, "queued").await["size"],
        1,
        "the failed re-queue must have dropped the old entry"
    );
}

#[tokio::test]
async fn ranked_room_aborts_when_matched_players_never_join() {
    let secret = "test-secret";
    // A dedicated server with a very short lobby grace so the abort path
    // runs in test time.
    let url = spawn_ranked_server_with_grace(secret, std::time::Duration::from_secs(1)).await;

    let mut a = connect(&url).await;
    send(
        &mut a,
        json!({"type": "queue_ranked", "auth": {"token": hs256_token(secret, "ua")}}),
    )
    .await;
    let mut b = connect(&url).await;
    send(
        &mut b,
        json!({"type": "queue_ranked", "auth": {"token": hs256_token(secret, "ub")}}),
    )
    .await;

    let code = recv_until(&mut a, "match_found").await["code"]
        .as_str()
        .expect("code")
        .to_string();
    recv_until(&mut b, "match_found").await;

    // Only a joins; b ignores the match. Below MIN_PLAYERS at the grace,
    // the room aborts (a is told) and dissolves (the code dies).
    send(
        &mut a,
        json!({"type": "join", "code": code, "auth": {"token": hs256_token(secret, "ua")}}),
    )
    .await;
    recv_until(&mut a, "joined").await;
    let err = recv_until(&mut a, "error").await;
    assert!(
        err["message"].as_str().unwrap().contains("aborted"),
        "the lone arrival learns the match died: {err}"
    );
    send(&mut a, json!({"type": "leave"})).await;
    send(
        &mut a,
        json!({"type": "join", "code": code, "auth": {"token": hs256_token(secret, "ua")}}),
    )
    .await;
    let err = recv_until(&mut a, "error").await;
    assert!(
        err["message"].as_str().unwrap().contains("no room"),
        "the aborted room must have dissolved: {err}"
    );
}

#[tokio::test]
async fn ranked_messages_error_cleanly_when_disabled() {
    // The default test server has no ranked service.
    let url = spawn_server().await;
    let mut socket = connect(&url).await;
    send(
        &mut socket,
        json!({"type": "queue_ranked", "auth": guest("ada")}),
    )
    .await;
    let err = recv_until(&mut socket, "error").await;
    assert!(err["message"].as_str().unwrap().contains("disabled"));
}

#[tokio::test]
async fn spectator_watches_without_a_seat_and_sees_nothing_private() {
    let url = spawn_server().await;

    // Two players set up and start a game.
    let mut host = connect(&url).await;
    send(&mut host, json!({"type": "create", "auth": guest("alice")})).await;
    let code = recv(&mut host).await["code"].as_str().unwrap().to_string();
    recv_until(&mut host, "joined").await;
    let mut bob = connect(&url).await;
    send(
        &mut bob,
        json!({"type": "join", "code": code, "auth": guest("bob")}),
    )
    .await;
    recv_until(&mut bob, "joined").await;
    send(&mut host, json!({"type": "start"})).await;
    recv_until(&mut host, "game_started").await;
    recv_until(&mut bob, "game_started").await;

    // A third connection watches by code: full context, no seat field.
    let mut watcher = connect(&url).await;
    send(
        &mut watcher,
        json!({"type": "spectate", "code": code, "auth": guest("carol")}),
    )
    .await;
    let spectating = recv_until(&mut watcher, "spectating").await;
    assert_eq!(spectating["code"], code.as_str());
    assert!(spectating.get("seat").is_none(), "a watcher holds no seat");
    assert!(
        spectating["view"]["players"].as_array().unwrap().len() == 2,
        "mid-game spectate carries the current view"
    );

    // A trade between the two players must not reach the watcher (ADR-0007
    // via ADR-0035): neither the event nor the pending offer in the view.
    send(
        &mut host,
        json!({"type": "cmd", "cmd": {"type": "propose_trade", "to": "guest:bob",
               "give_cash": 50, "give_tiles": [], "receive_cash": 0, "receive_tiles": []}}),
    )
    .await;
    let update = recv_until(&mut watcher, "update").await;
    let events = update["events"].as_array().unwrap();
    assert!(
        !events
            .iter()
            .any(|e| e["type"].as_str().unwrap_or("").starts_with("trade_")),
        "trade lifecycle must be filtered from the spectator feed: {events:?}"
    );
    assert!(
        update["view"]["pending_trades"]
            .as_array()
            .unwrap()
            .is_empty(),
        "no pending offer may appear in the spectator view"
    );

    // Game commands from a spectator are refused at the transport.
    send(
        &mut watcher,
        json!({"type": "cmd", "cmd": {"type": "end_turn"}}),
    )
    .await;
    let err = recv_until(&mut watcher, "error").await;
    assert!(err["message"].as_str().unwrap().contains("spectators"));

    // Bare spectate (no code) picks the human game on this server.
    let mut second = connect(&url).await;
    send(
        &mut second,
        json!({"type": "spectate", "auth": guest("dave")}),
    )
    .await;
    assert_eq!(
        recv_until(&mut second, "spectating").await["code"],
        code.as_str()
    );
}

#[tokio::test]
async fn showcase_supervisor_provides_a_bots_game_to_watch() {
    // A server with --showcase on and nobody playing: the supervisor's
    // first tick creates the bots room, and a bare spectate finds it.
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
        default_issuer: None,
        connections: AppState::connection_limiter(),
        ranked: None,
        guest_allowed: true,
        showcase: true,
    };
    parcello_server::showcase::spawn_showcase(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, game_router(state)).await.unwrap();
    });
    let url = format!("ws://{addr}/ws");

    // The supervisor ticks every 15s with the first tick immediate; give it
    // a moment, then watch whatever it made.
    let mut watcher = connect(&url).await;
    let mut spectating = None;
    for _ in 0..50 {
        send(
            &mut watcher,
            json!({"type": "spectate", "auth": guest("eve")}),
        )
        .await;
        let reply = recv(&mut watcher).await;
        if reply["type"] == "spectating" {
            spectating = Some(reply);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let spectating = spectating.expect("showcase room becomes watchable");
    let players = spectating["view"]["players"].as_array().expect("players");
    assert_eq!(players.len(), 4, "the showcase seats four bots");
    let seats = spectating["players"].as_array().expect("seat infos");
    assert!(
        seats.iter().all(|s| s["is_bot"] == true),
        "every showcase seat is a bot: {seats:?}"
    );
}
