//! `EdDSA` (Ed25519) identity-token verification against a JWKS (ADR-0009).
//!
//! The issuer is any OIDC provider that signs with Ed25519 (Rauthy is the
//! reference deployment); Parcello only verifies. Verification is
//! stateless: a background thread fetches and caches the public keys from
//! one or more JWKS URLs (redundant issuer instances), so `verify` only
//! reads the cache and never blocks the async executor.

use std::collections::HashMap;
use std::sync::{Arc, RwLock, mpsc};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use tracing::{info, warn};

use crate::auth::{Identity, decode_json};

/// Key-rotation pickup cadence when no unknown-kid poke arrives sooner.
const REFRESH_INTERVAL: Duration = Duration::from_mins(15);

type Keys = Arc<RwLock<HashMap<String, VerifyingKey>>>;

pub struct EdDsaVerifier {
    keys: Keys,
    poke: mpsc::Sender<()>,
    audience: Option<String>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    #[serde(default)]
    kid: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
    /// Unix seconds. Required: unbounded tokens are not accepted.
    exp: u64,
    #[serde(default)]
    aud: Option<Aud>,
}

/// RFC 7519: `aud` is a string or an array of strings.
#[derive(Deserialize)]
#[serde(untagged)]
enum Aud {
    One(String),
    Many(Vec<String>),
}

impl Aud {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Self::One(a) => a == expected,
            Self::Many(list) => list.iter().any(|a| a == expected),
        }
    }
}

/// Public display name from a verified token. Prefers `name`, then
/// `preferred_username`, then the opaque `sub` - through the shared
/// `auth::safe_display_name` chokepoint, so an email claim never leaks to the
/// table (see its docs; ADR-0009 privacy).
fn display_name(claims: &Claims) -> String {
    let candidates = [claims.name.as_deref(), claims.preferred_username.as_deref()]
        .into_iter()
        .flatten();
    crate::auth::safe_display_name(candidates, &claims.sub)
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kty: String,
    #[serde(default)]
    crv: String,
    #[serde(default)]
    kid: String,
    #[serde(default)]
    x: String,
}

/// Extracts every usable Ed25519 key; other key types are skipped (an
/// issuer may also serve RSA keys for other clients).
fn parse_jwks(json: &str) -> Vec<(String, VerifyingKey)> {
    let jwks: Jwks = match serde_json::from_str(json) {
        Ok(jwks) => jwks,
        Err(e) => {
            warn!(error = %e, "malformed JWKS document");
            return Vec::new();
        }
    };
    jwks.keys
        .iter()
        .filter_map(|k| {
            if k.kty != "OKP" || k.crv != "Ed25519" {
                return None;
            }
            let bytes: [u8; 32] = URL_SAFE_NO_PAD.decode(&k.x).ok()?.try_into().ok()?;
            let key = VerifyingKey::from_bytes(&bytes).ok()?;
            Some((k.kid.clone(), key))
        })
        .collect()
}

fn fetch_jwks(url: &str) -> Result<Vec<(String, VerifyingKey)>, String> {
    let mut resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok(parse_jwks(&body))
}

impl EdDsaVerifier {
    /// Starts the JWKS refresh thread. The first fetch happens immediately;
    /// failures retry on the next cycle, so a down issuer delays token
    /// logins but never blocks the server (guests are unaffected, and
    /// already-issued tokens verify against the cached keys).
    ///
    /// # Panics
    /// If the OS refuses to spawn the refresher thread (allocation failure
    /// at boot - not a recoverable state for the server).
    #[must_use]
    pub fn spawn(urls: Vec<String>, audience: Option<String>) -> Self {
        let keys: Keys = Arc::default();
        let (poke, rx) = mpsc::channel::<()>();
        let cache = Arc::clone(&keys);
        std::thread::Builder::new()
            .name("parcello-jwks".into())
            .spawn(move || {
                loop {
                    let mut fresh = HashMap::new();
                    for url in &urls {
                        match fetch_jwks(url) {
                            Ok(list) => fresh.extend(list),
                            Err(e) => warn!(url, error = %e, "JWKS fetch failed"),
                        }
                    }
                    // An empty fetch keeps the previous keys: a transient
                    // outage must not invalidate known-good keys.
                    if !fresh.is_empty() {
                        info!(keys = fresh.len(), "JWKS refreshed");
                        *cache.write().expect("jwks lock poisoned") = fresh;
                    }
                    if rx.recv_timeout(REFRESH_INTERVAL)
                        == Err(mpsc::RecvTimeoutError::Disconnected)
                    {
                        break; // Verifier dropped; stop refreshing.
                    }
                }
            })
            .expect("jwks thread spawns");
        Self {
            keys,
            poke,
            audience,
        }
    }

    /// # Errors
    /// Malformed/expired tokens, unknown key ids, wrong audience, or a
    /// signature that does not verify - all as client-facing text.
    ///
    /// # Panics
    /// If the JWKS cache lock was poisoned (a refresher-thread panic).
    pub fn verify(&self, token: &str) -> Result<Identity, String> {
        let mut parts = token.split('.');
        let (Some(h), Some(p), Some(s), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err("malformed token".into());
        };
        let header: Header = decode_json(h)?;
        if header.alg != "EdDSA" {
            return Err("unsupported token algorithm".into());
        }
        let sig: [u8; 64] = URL_SAFE_NO_PAD
            .decode(s)
            .map_err(|_| "malformed token signature")?
            .try_into()
            .map_err(|_| "malformed token signature")?;
        let sig = Signature::from_bytes(&sig);
        let msg = format!("{h}.{p}");

        let keys = self.keys.read().expect("jwks lock poisoned");
        let verified = match &header.kid {
            Some(kid) => keys
                .get(kid)
                .map(|k| k.verify(msg.as_bytes(), &sig).is_ok()),
            // No kid: small key sets, try them all.
            None => Some(
                keys.values()
                    .any(|k| k.verify(msg.as_bytes(), &sig).is_ok()),
            ),
        };
        drop(keys);
        match verified {
            Some(true) => {}
            Some(false) => return Err("invalid token signature".into()),
            None => {
                // Unknown kid is usually key rotation: refresh, ask to retry.
                let _ = self.poke.send(());
                return Err("unknown signing key; retry shortly".into());
            }
        }

        let claims: Claims = decode_json(p)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "server clock error")?
            .as_secs();
        if claims.exp <= now {
            return Err("token expired".into());
        }
        if let Some(expected) = &self.audience {
            let ok = claims
                .aud
                .as_ref()
                .is_some_and(|aud| aud.contains(expected));
            if !ok {
                return Err("token audience mismatch".into());
            }
        }

        Ok(Identity {
            player_id: format!("id:{}", claims.sub),
            name: display_name(&claims),
            spoofable: false,
        })
    }

    #[cfg(test)]
    fn insert_key(&self, kid: &str, key: VerifyingKey) {
        self.keys
            .write()
            .expect("jwks lock poisoned")
            .insert(kid.to_string(), key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sign(key: &SigningKey, header: &str, claims: &str) -> String {
        let h = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let p = URL_SAFE_NO_PAD.encode(claims.as_bytes());
        let sig = key.sign(format!("{h}.{p}").as_bytes());
        let s = URL_SAFE_NO_PAD.encode(sig.to_bytes());
        format!("{h}.{p}.{s}")
    }

    fn far_future() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            + 3600
    }

    fn verifier_with_test_key() -> EdDsaVerifier {
        let v = EdDsaVerifier::spawn(Vec::new(), None);
        v.insert_key("k1", test_key().verifying_key());
        v
    }

    #[test]
    fn valid_token_yields_a_non_spoofable_identity() {
        let v = verifier_with_test_key();
        let token = sign(
            &test_key(),
            r#"{"alg":"EdDSA","kid":"k1"}"#,
            &format!(
                r#"{{"sub":"u_9f2","preferred_username":"Vian","exp":{}}}"#,
                far_future()
            ),
        );
        let id = v.verify(&token).expect("valid token");
        assert_eq!(id.player_id, "id:u_9f2");
        assert_eq!(id.name, "Vian");
        assert!(!id.spoofable);
    }

    #[test]
    fn display_name_never_leaks_an_email() {
        let claims = |name: Option<&str>, pref: Option<&str>, sub: &str| Claims {
            sub: sub.into(),
            name: name.map(Into::into),
            preferred_username: pref.map(Into::into),
            exp: 0,
            aud: None,
        };
        // An email in `name` is skipped in favour of the username handle.
        assert_eq!(
            display_name(&claims(Some("ada@example.com"), Some("ada"), "u1")),
            "ada"
        );
        // Email in both name and preferred_username: fall back to opaque sub.
        assert_eq!(
            display_name(&claims(Some("ada@x.com"), Some("ada@x.com"), "u1")),
            "u1"
        );
        // A normal display name is used as-is.
        assert_eq!(display_name(&claims(Some("Ada"), None, "u1")), "Ada");
        // Even an email-shaped subject yields only the local part - no domain.
        let name = display_name(&claims(Some("ada@x.com"), None, "ada@x.com"));
        assert!(!name.contains('@'), "no address may leak: {name}");
    }

    #[test]
    fn verify_does_not_surface_an_email_display_name() {
        let v = verifier_with_test_key();
        let token = sign(
            &test_key(),
            r#"{"alg":"EdDSA","kid":"k1"}"#,
            &format!(
                r#"{{"sub":"u_9f2","name":"player@thevhome.com","exp":{}}}"#,
                far_future()
            ),
        );
        let id = v.verify(&token).expect("valid token");
        assert!(!id.name.contains('@'), "email leaked as name: {}", id.name);
        assert_eq!(id.name, "u_9f2", "falls back to the opaque sub");
    }

    #[test]
    fn tampered_expired_or_unknown_kid_tokens_are_rejected() {
        let v = verifier_with_test_key();
        let good = sign(
            &test_key(),
            r#"{"alg":"EdDSA","kid":"k1"}"#,
            &format!(r#"{{"sub":"a","exp":{}}}"#, far_future()),
        );
        assert!(v.verify(&(good + "x")).is_err(), "tampered sig");

        let expired = sign(
            &test_key(),
            r#"{"alg":"EdDSA","kid":"k1"}"#,
            r#"{"sub":"a","exp":1}"#,
        );
        assert!(v.verify(&expired).is_err(), "expired");

        let wrong_key = SigningKey::from_bytes(&[9u8; 32]);
        let forged = sign(
            &wrong_key,
            r#"{"alg":"EdDSA","kid":"k1"}"#,
            &format!(r#"{{"sub":"a","exp":{}}}"#, far_future()),
        );
        assert!(v.verify(&forged).is_err(), "wrong key");

        let unknown_kid = sign(
            &test_key(),
            r#"{"alg":"EdDSA","kid":"other"}"#,
            &format!(r#"{{"sub":"a","exp":{}}}"#, far_future()),
        );
        assert!(v.verify(&unknown_kid).is_err(), "unknown kid");
    }

    #[test]
    fn audience_is_enforced_when_configured() {
        let v = EdDsaVerifier::spawn(Vec::new(), Some("parcello".into()));
        v.insert_key("k1", test_key().verifying_key());
        let mint = |aud: &str| {
            sign(
                &test_key(),
                r#"{"alg":"EdDSA","kid":"k1"}"#,
                &format!(r#"{{"sub":"a","exp":{},{aud}}}"#, far_future()),
            )
        };
        assert!(v.verify(&mint(r#""aud":"parcello""#)).is_ok());
        assert!(v.verify(&mint(r#""aud":["x","parcello"]"#)).is_ok());
        assert!(v.verify(&mint(r#""aud":"other""#)).is_err());
        // Missing aud entirely also fails when an audience is required.
        let no_aud = sign(
            &test_key(),
            r#"{"alg":"EdDSA","kid":"k1"}"#,
            &format!(r#"{{"sub":"a","exp":{}}}"#, far_future()),
        );
        assert!(v.verify(&no_aud).is_err());
    }

    /// End to end through the fetch path: serve a real JWKS over HTTP and
    /// let the refresh thread pick it up.
    #[tokio::test(flavor = "multi_thread")]
    async fn jwks_is_fetched_from_a_live_endpoint() {
        let key = test_key().verifying_key();
        let jwks = format!(
            r#"{{"keys":[{{"kty":"OKP","crv":"Ed25519","kid":"live","x":"{}"}},
                       {{"kty":"RSA","kid":"ignored","n":"x","e":"AQAB"}}]}}"#,
            URL_SAFE_NO_PAD.encode(key.as_bytes())
        );
        let app = axum::Router::new().route(
            "/jwks",
            axum::routing::get(move || {
                let jwks = jwks.clone();
                async move { jwks }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        let v = EdDsaVerifier::spawn(vec![format!("http://{addr}/jwks")], None);
        let token = sign(
            &test_key(),
            r#"{"alg":"EdDSA","kid":"live"}"#,
            &format!(r#"{{"sub":"net","exp":{}}}"#, far_future()),
        );
        // The refresh thread fetches asynchronously; poll briefly.
        for _ in 0..50 {
            if v.verify(&token).is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("JWKS was never fetched from the live endpoint");
    }
}
