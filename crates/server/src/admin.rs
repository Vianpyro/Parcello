//! Admin feedback console: authorisation and routes (ADR-0038).
//!
//! Read-only. The credential is the ordinary OIDC id token the game client
//! already obtains, verified by the same `IdentityVerifier` as a `join`, so
//! `exp`/`aud`/JWKS handling has exactly one implementation. What is added
//! here is *authorisation*: an operator-set allowlist of subjects, which is
//! deliberately not an issuer-side role claim - community servers share an
//! identity provider (docs/deployment.md), so a role in the token would
//! grant admin on every server on that issuer, including other people's.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use parcello_protocol::AuthPayload;
use serde::Serialize;

use crate::AppState;
use crate::feedback::{FEEDBACK_QUERY_LIMIT, FeedbackEntry};

/// The console, compiled into the binary (ADR-0038): one file, always
/// present, no deployment step. The Flutter bundle stays on disk (ADR-0025)
/// because it is large and rebuilt independently; this is neither.
const CONSOLE_HTML: &str = include_str!("../assets/admin.html");

/// Identity schemes `Identity::player_id` can carry (auth.rs, eddsa.rs).
/// A player id is always `<scheme>:<subject>`; `--admin` accepts either
/// form, so an operator can paste a bare OIDC subject.
const SCHEMES: [&str; 3] = ["id:", "hs256:", "guest:"];

/// Parses one `--admin` value, or the whole `PARCELLO_ADMIN_SUBS` env form:
/// subjects separated by commas or whitespace, empty entries ignored.
///
/// Each value is normalised to a full player id. A bare subject means the
/// real (`EdDSA`) scheme, because that is the only one that can ever be an
/// administrator: HS256 is the deprecated stopgap (ADR-0003) and guests are
/// refused before the allowlist is read. A `guest:` value is therefore
/// dropped outright - it could never match, and keeping a dead entry would
/// let an operator believe they had granted access.
#[must_use]
pub fn parse_admin_subs(raw: &str) -> Vec<String> {
    raw.split([',', ' ', '\t', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|value| {
            if value.starts_with("guest:") {
                tracing::warn!(
                    value,
                    "ignoring guest identity in --admin: guests are never administrators"
                );
                return None;
            }
            Some(if SCHEMES.iter().any(|s| value.starts_with(s)) {
                value.to_owned()
            } else {
                format!("id:{value}")
            })
        })
        .collect()
}

#[derive(Serialize)]
struct ApiError {
    error: &'static str,
}

fn json_error(status: StatusCode, error: &'static str) -> Response {
    (status, Json(ApiError { error })).into_response()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(ApiError {
            error: "sign in with an administrator account",
        }),
    )
        .into_response()
}

/// Extracts the token from an `Authorization: Bearer <token>` header.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

/// The response that refuses this request, or `None` when it may proceed.
///
/// Three refusals, in the order that leaks the least:
///
/// 1. no allowlist configured - the whole surface is 404, so a server
///    without `--admin` does not advertise a console at all;
/// 2. missing or invalid token - 401;
/// 3. a valid token that is spoofable (guests, ADR-0003/0008) or absent
///    from the allowlist - 403. Guests are refused before the allowlist is
///    consulted, the same rule the ranked queue applies (ADR-0034): a
///    forgeable identity must never reach other players' words.
///
/// The subject comparison is an ordinary hash lookup, not constant-time:
/// the value compared is not a secret, and reaching it already requires a
/// token this server's issuer signed for that subject (unlike the
/// per-seat reconnect tokens of ADR-0008, which are secrets).
fn refusal(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    if state.admins.is_empty() {
        return Some(StatusCode::NOT_FOUND.into_response());
    }
    let Some(token) = bearer(headers) else {
        return Some(unauthorized());
    };
    let auth = AuthPayload {
        token: Some(token.to_owned()),
        ..AuthPayload::default()
    };
    let Ok(identity) = state.verifier.verify(&auth) else {
        return Some(unauthorized());
    };
    (identity.spoofable || !state.admins.contains(&identity.player_id)).then(|| {
        json_error(
            StatusCode::FORBIDDEN,
            "this account is not an administrator of this server",
        )
    })
}

/// `GET /admin` - the console shell. Unauthenticated by design: it holds no
/// data, every value arrives from the authenticated API below.
pub async fn console(State(state): State<AppState>) -> Response {
    if state.admins.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    Html(CONSOLE_HTML).into_response()
}

#[derive(Serialize)]
struct FeedbackResponse {
    /// False when the server runs without `--history`: there is no database
    /// to read. The console says so instead of showing an empty list, which
    /// would be indistinguishable from "nobody has answered yet".
    configured: bool,
    entries: Vec<FeedbackEntry>,
}

/// `GET /admin/api/feedback` - every stored answer, newest first.
pub async fn feedback(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(refused) = refusal(&state, &headers) {
        return refused;
    }
    let Some(query) = state.feedback.clone() else {
        return Json(FeedbackResponse {
            configured: false,
            entries: Vec::new(),
        })
        .into_response();
    };
    // rusqlite is blocking; keep it off the async executor.
    let read = tokio::task::spawn_blocking(move || query.recent(FEEDBACK_QUERY_LIMIT)).await;
    match read {
        Ok(Ok(entries)) => Json(FeedbackResponse {
            configured: true,
            entries,
        })
        .into_response(),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "feedback query failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the history database",
            )
        }
        Err(e) => {
            tracing::warn!(error = %e, "feedback query task failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the history database",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_form_accepts_commas_and_whitespace() {
        assert_eq!(
            parse_admin_subs("a, b\tc\nd , ,e"),
            vec!["id:a", "id:b", "id:c", "id:d", "id:e"]
        );
        assert!(parse_admin_subs("   ").is_empty());
        assert!(parse_admin_subs("").is_empty());
    }

    /// A bare subject means the real scheme; an explicit one is kept as
    /// given, including a subject that contains colons of its own.
    #[test]
    fn bare_subjects_normalise_to_the_eddsa_scheme() {
        assert_eq!(parse_admin_subs("3f9a"), vec!["id:3f9a"]);
        assert_eq!(parse_admin_subs("id:3f9a"), vec!["id:3f9a"]);
        assert_eq!(parse_admin_subs("hs256:u1"), vec!["hs256:u1"]);
        assert_eq!(parse_admin_subs("disc:42"), vec!["id:disc:42"]);
    }

    #[test]
    fn guests_are_dropped_rather_than_stored_as_dead_entries() {
        assert!(parse_admin_subs("guest:vian").is_empty());
        assert_eq!(parse_admin_subs("guest:vian, 3f9a"), vec!["id:3f9a"]);
    }

    #[test]
    fn bearer_is_scheme_insensitive_and_rejects_junk() {
        let with = |v: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::AUTHORIZATION, v.parse().expect("header value"));
            h
        };
        assert_eq!(bearer(&with("Bearer tok")), Some("tok"));
        assert_eq!(bearer(&with("bearer tok")), Some("tok"));
        assert_eq!(bearer(&with("BEARER  tok ")), Some("tok"));
        assert_eq!(bearer(&with("Basic tok")), None);
        assert_eq!(bearer(&with("Bearer ")), None);
        assert_eq!(bearer(&with("tok")), None);
        assert_eq!(bearer(&HeaderMap::new()), None);
    }

    /// The page must not carry data, credentials, or a network origin of
    /// its own: it is a shell served to anyone who knows the path.
    #[test]
    fn the_console_shell_is_self_contained() {
        assert!(CONSOLE_HTML.contains("<!doctype html>"));
        assert!(
            !CONSOLE_HTML.contains("//cdn.") && !CONSOLE_HTML.contains("https://fonts."),
            "the console must make no external requests"
        );
    }
}
