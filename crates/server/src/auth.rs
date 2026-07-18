//! Session-layer identity verification.
//!
//! ADR-0003: the Global Identity Service does not exist yet. Verification is
//! gated behind `IdentityVerifier` so the future service (asymmetric JWT
//! verification over HTTP) slots in without touching room or transport code.
//! MVP modes: HS256 shared-secret tokens and/or explicit insecure guests.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;

use parcello_protocol::AuthPayload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Stable global id (`hs256:<sub>` or `guest:<name>`).
    pub player_id: String,
    pub name: String,
    /// True when nothing cryptographic backs this identity (guests). The
    /// room then requires the seat's reconnect token to rejoin (ADR-0008).
    pub spoofable: bool,
}

pub trait IdentityVerifier: Send + Sync {
    /// # Errors
    /// Returns a human-readable reason when the payload carries no usable
    /// credentials or the token fails verification; the transport forwards
    /// it to the client verbatim.
    fn verify(&self, auth: &AuthPayload) -> Result<Identity, String>;
}

/// Tries a JWT first (if configured), then guest fallback (if enabled).
/// Tokens are dispatched by their header `alg`: `EdDSA` is the supported
/// path (ADR-0009), HS256 the deprecated stopgap (ADR-0003).
pub struct CompositeVerifier {
    eddsa: Option<crate::eddsa::EdDsaVerifier>,
    hs256: Option<Hs256Verifier>,
    allow_guests: bool,
}

impl CompositeVerifier {
    pub fn new(
        eddsa: Option<crate::eddsa::EdDsaVerifier>,
        jwt_secret: Option<String>,
        allow_guests: bool,
    ) -> Self {
        Self {
            eddsa,
            hs256: jwt_secret.map(Hs256Verifier::new),
            allow_guests,
        }
    }
}

#[derive(Deserialize)]
struct AlgOnly {
    alg: String,
}

impl IdentityVerifier for CompositeVerifier {
    fn verify(&self, auth: &AuthPayload) -> Result<Identity, String> {
        if let Some(token) = &auth.token {
            let header = token.split('.').next().unwrap_or_default();
            let alg: AlgOnly = decode_json(header)?;
            return match alg.alg.as_str() {
                "EdDSA" => self
                    .eddsa
                    .as_ref()
                    .ok_or("this server has no identity provider configured (--identity-url)")?
                    .verify(token),
                "HS256" => self
                    .hs256
                    .as_ref()
                    .ok_or("this server does not accept HS256 tokens (no JWT secret)")?
                    .verify(token),
                other => Err(format!("unsupported token algorithm: {other}")),
            };
        }
        if let Some(name) = &auth.guest_name {
            if !self.allow_guests {
                return Err("guest access disabled; provide a token".into());
            }
            let name = sanitize_guest_name(name)?;
            return Ok(Identity {
                player_id: format!("guest:{}", name.to_lowercase()),
                name,
                spoofable: true,
            });
        }
        Err("auth payload must contain a token or a guest_name".into())
    }
}

/// A public display name that never leaks an email address. This name is
/// broadcast to every seat, but an OIDC provider (Rauthy included) commonly
/// fills identity claims (`name`, `preferred_username`) with the account email
/// when no separate display name is set - surfacing it would expose every
/// signed-in player's address to the table. Returns the first candidate that
/// is non-blank and not email-shaped (contains `@`); otherwise the opaque
/// `sub`, and if even that is an address, only its local part - so no full
/// email is ever shown. Trimmed to the same 24-char budget as guest names.
/// The single privacy chokepoint for every token verifier (ADR-0009).
pub(crate) fn safe_display_name<'a>(
    candidates: impl IntoIterator<Item = &'a str>,
    sub: &str,
) -> String {
    let usable = |s: &str| {
        let s = s.trim();
        (!s.is_empty() && !s.contains('@')).then(|| s.to_string())
    };
    let name = candidates
        .into_iter()
        .find_map(usable)
        .or_else(|| usable(sub))
        .unwrap_or_else(|| {
            sub.split('@')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("player")
                .to_string()
        });
    name.chars().take(24).collect()
}

/// Guest names are identity in insecure mode: same name = same seat on
/// rejoin. Mid-game seats are shielded by reconnect tokens (ADR-0008),
/// but names remain impersonable at first join. LAN/testing only.
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
    #[must_use]
    pub const fn new(secret: String) -> Self {
        Self {
            secret: secret.into_bytes(),
        }
    }

    fn verify(&self, token: &str) -> Result<Identity, String> {
        let mut parts = token.split('.');
        let (Some(h), Some(p), Some(s), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err("malformed token".into());
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
            name: safe_display_name([claims.name.as_str()], &claims.sub),
            spoofable: false,
        })
    }
}

/// # Errors
/// When `part` is not base64url or not the expected JSON shape.
pub fn decode_json<T: serde::de::DeserializeOwned>(part: &str) -> Result<T, String> {
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
        let v = CompositeVerifier::new(None, Some("s3cret".into()), false);
        let id = v
            .verify(&AuthPayload {
                token: Some(token),
                guest_name: None,
                reconnect: None,
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
        let v = CompositeVerifier::new(None, Some("s3cret".into()), false);

        let wrong_key = sign(
            "other",
            &format!(r#"{{"sub":"a","name":"A","exp":{}}}"#, far_future()),
        );
        assert!(
            v.verify(&AuthPayload {
                token: Some(wrong_key),
                guest_name: None,
                reconnect: None,
            })
            .is_err()
        );

        let expired = sign("s3cret", r#"{"sub":"a","name":"A","exp":1}"#);
        assert!(
            v.verify(&AuthPayload {
                token: Some(expired),
                guest_name: None,
                reconnect: None,
            })
            .is_err()
        );

        assert!(
            v.verify(&AuthPayload {
                token: Some(good + "x"),
                guest_name: None,
                reconnect: None,
            })
            .is_err()
        );
    }

    #[test]
    fn guest_mode_gates_and_sanitizes() {
        let open = CompositeVerifier::new(None, None, true);
        let id = open
            .verify(&AuthPayload {
                token: None,
                guest_name: Some("Vian_42".into()),
                reconnect: None,
            })
            .unwrap();
        assert_eq!(id.player_id, "guest:vian_42");

        assert!(
            open.verify(&AuthPayload {
                token: None,
                guest_name: Some("bad name!".into()),
                reconnect: None,
            })
            .is_err()
        );

        let closed = CompositeVerifier::new(None, None, false);
        assert!(
            closed
                .verify(&AuthPayload {
                    token: None,
                    guest_name: Some("Vian".into()),
                    reconnect: None,
                })
                .is_err()
        );
    }
}
