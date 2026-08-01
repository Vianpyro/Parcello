//! Admin console integration tests (ADR-0038): the real router on an
//! ephemeral port, spoken to over HTTP.
//!
//! The point of this file is the refusal matrix. Authorisation is the only
//! thing standing between an untrusted request and other players' words, so
//! every branch of it is asserted here rather than in a unit test that
//! could drift from the wired router.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use parcello_server::auth::CompositeVerifier;
use parcello_server::feedback::{FeedbackQuery, SqliteFeedback};
use parcello_server::history::{GameHistory, MemoryHistory, SqliteHistory};
use parcello_server::room::Rooms;
use parcello_server::{AppState, game_router};
use sha2::Sha256;

const SECRET: &str = "s3cret";

fn base_state(admins: &[&str], feedback: Option<Arc<dyn FeedbackQuery>>) -> AppState {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mods_dir = manifest
        .join("../../mods")
        .canonicalize()
        .expect("mods dir");
    let resolved = parcello_mods::resolve(&mods_dir, &["base".to_string()]).expect("base resolves");
    AppState {
        rooms: Rooms::default(),
        content: Arc::new(resolved),
        mods_dir: Arc::new(mods_dir),
        // HS256 stands in for the real EdDSA path: `CompositeVerifier`
        // produces the same `Identity` shape, and only its `player_id` and
        // `spoofable` flag matter to the guard.
        verifier: Arc::new(CompositeVerifier::new(None, Some(SECRET.to_string()), true)),
        history: Arc::new(MemoryHistory::new()),
        turn_timeout: None,
        time_bank: None,
        game_timeout: None,
        default_issuer: None,
        connections: AppState::connection_limiter(),
        ranked: None,
        guest_allowed: true,
        showcase: false,
        admins: Arc::new(
            admins
                .iter()
                .map(|a| (*a).to_string())
                .collect::<HashSet<_>>(),
        ),
        feedback,
    }
}

async fn spawn(state: AppState) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, game_router(state))
            .await
            .expect("serve");
    });
    format!("http://{addr}")
}

fn token(sub: &str, expires_in: i64) -> String {
    let exp = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs(),
    )
    .expect("epoch fits")
        + expires_in;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload =
        URL_SAFE_NO_PAD.encode(format!(r#"{{"sub":"{sub}","name":"admin","exp":{exp}}}"#));
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).expect("hmac key");
    mac.update(format!("{header}.{payload}").as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{header}.{payload}.{sig}")
}

/// `ureq` is blocking; the server lives on the multi-thread runtime.
async fn get(url: String, bearer: Option<String>) -> (u16, String) {
    tokio::task::spawn_blocking(move || {
        let mut req = ureq::get(&url);
        if let Some(bearer) = bearer {
            req = req.header("Authorization", &format!("Bearer {bearer}"));
        }
        match req.call() {
            Ok(mut resp) => (
                resp.status().as_u16(),
                resp.body_mut().read_to_string().unwrap_or_default(),
            ),
            Err(ureq::Error::StatusCode(code)) => (code, String::new()),
            Err(e) => panic!("request failed: {e}"),
        }
    })
    .await
    .expect("request task")
}

#[tokio::test(flavor = "multi_thread")]
async fn without_admins_the_console_does_not_exist() {
    let base = spawn(base_state(&[], None)).await;

    let (page, _) = get(format!("{base}/admin"), None).await;
    assert_eq!(page, 404, "the shell must not advertise a disabled console");

    // 404, not 401: an unconfigured server reveals nothing about the
    // feature, even to a caller holding a perfectly good token.
    let (api, _) = get(
        format!("{base}/admin/api/feedback"),
        Some(token("boss", 3600)),
    )
    .await;
    assert_eq!(api, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shell_is_public_but_the_data_is_not() {
    let base = spawn(base_state(&["hs256:boss"], None)).await;

    let (page, body) = get(format!("{base}/admin"), None).await;
    assert_eq!(page, 200);
    assert!(body.contains("<!doctype html>"));
    assert!(
        !body.contains("hs256:boss"),
        "the shell must carry no identity or data"
    );

    let (api, _) = get(format!("{base}/admin/api/feedback"), None).await;
    assert_eq!(api, 401, "no credential, no answers");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_listed_unspoofable_identity_reads_the_answers() {
    let base = spawn(base_state(&["hs256:boss"], None)).await;
    let api = format!("{base}/admin/api/feedback");

    let (listed, body) = get(api.clone(), Some(token("boss", 3600))).await;
    assert_eq!(listed, 200);
    assert!(
        body.contains("\"configured\":false"),
        "no --history: {body}"
    );

    let (stranger, _) = get(api.clone(), Some(token("someone-else", 3600))).await;
    assert_eq!(stranger, 403, "a valid token is not an authorisation");

    let (expired, _) = get(api.clone(), Some(token("boss", -3600))).await;
    assert_eq!(expired, 401, "expiry is enforced on the console too");

    let (garbage, _) = get(api.clone(), Some("not-a-token".to_string())).await;
    assert_eq!(garbage, 401);

    // Forged signature, right subject: the guard must never trust the
    // claims without the verifier.
    let mut forged = token("boss", 3600);
    forged.truncate(forged.rfind('.').expect("signature") + 1);
    forged.push_str("AAAA");
    let (tampered, _) = get(api, Some(forged)).await;
    assert_eq!(tampered, 401);
}

/// A guest identity is unforgeable-by-nobody: even if an operator managed
/// to name one, `spoofable` must refuse it before the allowlist is read.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_identity_is_never_an_administrator() {
    let state = base_state(&["guest:boss"], None);
    let base = spawn(state).await;

    let (status, _) = tokio::task::spawn_blocking({
        let url = format!("{base}/admin/api/feedback");
        move || match ureq::get(&url).header("Authorization", "Bearer ").call() {
            Ok(resp) => (resp.status().as_u16(), String::new()),
            Err(ureq::Error::StatusCode(code)) => (code, String::new()),
            Err(e) => panic!("request failed: {e}"),
        }
    })
    .await
    .expect("request task");
    assert_eq!(status, 401, "an empty bearer is no credential at all");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_administrator_reads_answers_joined_to_their_games() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.db");

    let history = SqliteHistory::open(&path).expect("open history");
    history.record_start("ABCDE", &["hs256:boss".into(), "hs256:rival".into()], 7);
    history.record_end("ABCDE", Some("hs256:boss"));
    history.record_feedback(
        "ABCDE",
        "hs256:boss",
        5,
        Some("the auction window is the game"),
    );
    drop(history);

    let query: Arc<dyn FeedbackQuery> = Arc::new(SqliteFeedback::open(&path).expect("open query"));
    let base = spawn(base_state(&["hs256:boss"], Some(query))).await;

    let (status, body) = get(
        format!("{base}/admin/api/feedback"),
        Some(token("boss", 3600)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("\"configured\":true"));
    assert!(body.contains("the auction window is the game"));
    assert!(body.contains("\"outcome\":\"won\""));
    assert!(body.contains("\"seats\":2"));
    assert!(body.contains("\"room\":\"ABCDE\""));
}
