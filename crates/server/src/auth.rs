//! Session-layer identity verification.
//!
//! ADR-0003: the Global Identity Service does not exist yet. Verification is
//! gated behind `IdentityVerifier` so the future service (asymmetric JWT
//! verification over HTTP) slots in without touching room or transport code.
//! MVP modes: HS256 shared-secret tokens and/or explicit insecure guests.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use parcello_protocol::AuthPayload;

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    /// Stable global id ("hs256:<sub>" or "guest:<name>").
    pub player_id: String,
    pub name: String,
}

pub trait IdentityVerifier: Send + Sync {
    fn verify(&self, auth: &AuthPayload) -> Result<Identity, String>;
}

/// Tries a JWT first (if configured), then guest fallback (if enabled).
pub struct CompositeVerifier {
    hs256: Option<Hs256Verifier>,
    allow_guests: bool,
}

impl CompositeVerifier {
    pub fn new(jwt_secret: Option<String>, allow_guests: bool) -> Self {
        Self {
            hs256: jwt_secret.map(Hs256Verifier::new),
            allow_guests,
        }
    }
}

impl IdentityVerifier for CompositeVerifier {
    fn verify(&self, auth: &AuthPayload) -> Result<Identity, String> {
        if let Some(token) = &auth.token {
            let verifier = self
                .hs256
                .as_ref()
                .ok_or("this server does not accept tokens (no JWT secret configured)")?;
            return verifier.verify(token);
        }
        if let Some(name) = &auth.guest_name {
            if !self.allow_guests {
                return Err("guest access disabled; provide a token".into());
            }
            let name = sanitize_guest_name(name)?;
            return Ok(Identity {
                player_id: format!("guest:{}", name.to_lowercase()),
                name,
            });
        }
        Err("auth payload must contain a token or a guest_name".into())
    }
}

/// Guest names are identity in insecure mode: same name = same seat on
/// rejoin, and nothing prevents impersonation. LAN/testing only.
fn sanitize_guest_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() || name.len() > 24 {
        return Err("guest name must be 1-24 characters".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("guest name must be ASCII alphanumeric, '_' or '-'".into());
    }
    Ok(name.to_string())
}

/// HS256 JWT verification without a JWT crate: header.payload.signature,
/// base64url, HMAC-SHA256 with constant-time comparison via `hmac`.
pub struct Hs256Verifier {
    secret: Vec<u8>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    name: String,
    /// Unix seconds. Required: unbounded tokens are not accepted.
    exp: u64,
}

impl Hs256Verifier {
    pub fn new(secret: String) -> Self {
        Self {
            secret: secret.into_bytes(),
        }
    }

    fn verify(&self, token: &str) -> Result<Identity, String> {
        let mut parts = token.split('.');
        let (h, p, s) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(h), Some(p), Some(s), None) => (h, p, s),
            _ => return Err("malformed token".into()),
        };

        let header: Header = decode_json(h)?;
        if header.alg != "HS256" {
            return Err("unsupported token algorithm".into());
        }

        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret)
            .map_err(|_| "server JWT secret is invalid")?;
        mac.update(format!("{h}.{p}").as_bytes());
        let sig = URL_SAFE_NO_PAD
            .decode(s)
            .map_err(|_| "malformed token signature")?;
        mac.verify_slice(&sig)
            .map_err(|_| "invalid token signature")?;

        let claims: Claims = decode_json(p)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "server clock error")?
            .as_secs();
        if claims.exp <= now {
            return Err("token expired".into());
        }

        Ok(Identity {
            player_id: format!("hs256:{}", claims.sub),
            name: claims.name,
        })
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(part: &str) -> Result<T, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|_| "malformed token encoding")?;
    serde_json::from_slice(&bytes).map_err(|_| "malformed token claims".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, claims: &str) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.as_bytes());
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{header}.{payload}").as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{header}.{payload}.{sig}")
    }

    fn far_future() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600
    }

    #[test]
    fn valid_token_yields_identity() {
        let token = sign(
            "s3cret",
            &format!(
                r#"{{"sub":"disc:42","name":"Vian","exp":{}}}"#,
                far_future()
            ),
        );
        let v = CompositeVerifier::new(Some("s3cret".into()), false);
        let id = v
            .verify(&AuthPayload {
                token: Some(token),
                guest_name: None,
            })
            .unwrap();
        assert_eq!(id.player_id, "hs256:disc:42");
        assert_eq!(id.name, "Vian");
    }

    #[test]
    fn tampered_or_expired_tokens_are_rejected() {
        let good = sign(
            "s3cret",
            &format!(r#"{{"sub":"a","name":"A","exp":{}}}"#, far_future()),
        );
        let v = CompositeVerifier::new(Some("s3cret".into()), false);

        let wrong_key = sign(
            "other",
            &format!(r#"{{"sub":"a","name":"A","exp":{}}}"#, far_future()),
        );
        assert!(v
            .verify(&AuthPayload {
                token: Some(wrong_key),
                guest_name: None
            })
            .is_err());

        let expired = sign("s3cret", r#"{"sub":"a","name":"A","exp":1}"#);
        assert!(v
            .verify(&AuthPayload {
                token: Some(expired),
                guest_name: None
            })
            .is_err());

        assert!(v
            .verify(&AuthPayload {
                token: Some(good + "x"),
                guest_name: None
            })
            .is_err());
    }

    #[test]
    fn guest_mode_gates_and_sanitizes() {
        let open = CompositeVerifier::new(None, true);
        let id = open
            .verify(&AuthPayload {
                token: None,
                guest_name: Some("Vian_42".into()),
            })
            .unwrap();
        assert_eq!(id.player_id, "guest:vian_42");

        assert!(open
            .verify(&AuthPayload {
                token: None,
                guest_name: Some("bad name!".into()),
            })
            .is_err());

        let closed = CompositeVerifier::new(None, false);
        assert!(closed
            .verify(&AuthPayload {
                token: None,
                guest_name: Some("Vian".into()),
            })
            .is_err());
    }
}
