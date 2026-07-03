# ADR-0009: Identity Service design (self-hosted, redundant, optional)

Status: accepted (verifier implemented server-side; issuer = existing
OIDC provider, see amendment below)

## Context
The interim auth (ADR-0003) leaves two gaps: guest names are impersonable
at first join, and the HS256 stopgap requires sharing a secret with every
game server. The service will be self-hosted by the project owner, with
these requirements, in order:

1. **Security first.**
2. **Privacy second.**
3. Playing without an account must always be possible. Accounts only add
   nice-to-haves (stats, display name continuity) - never gate play.
4. Redundant: independent instances in different locations, so one power
   outage never blocks logins.

## Decision

**Accounts stay optional, forever.** Guest mode remains a first-class
path in the game server (`--insecure-guest` today; reconnect tokens
already protect guest seats mid-game, ADR-0008). The `IdentityVerifier`
composite tries token auth first and falls back to guests; that shape
does not change.

**Stateless verification via asymmetric signatures.** The service issues
short-lived (24h) EdDSA (Ed25519) JWTs with claims `{sub, name, exp, kid}`
only. Game servers verify against the service's public keys fetched from
a JWKS endpoint (`--identity-url`, repeatable) - they never hold secrets
and never call the service per login. This kills the HS256
shared-secret model.

**Redundancy falls out of statelessness.** Because verification needs
only public keys:
- Any number of issuer instances can run behind different addresses;
  each game server tries its `--identity-url` list in order and caches
  the JWKS (refresh on unknown `kid`, plus a TTL).
- Instances share the signing keypair (replicated once, offline) or each
  has its own key with the JWKS serving the union - both work; the union
  survives a compromised instance better (revoke one `kid`).
- Already-issued tokens keep working even if every instance is down,
  until `exp`. Combined with guest mode, an identity outage can never
  prevent playing.
- The accounts database is small (credential hash, sub, display name,
  stats) and syncs with ordinary tools (SQLite + Litestream, or Postgres
  replication) - nothing in the game protocol depends on how.

**Privacy by data minimization.**
- `sub` is a random opaque id generated at signup - never an email, never
  derived from one.
- Tokens carry no PII beyond the chosen display name.
- Game servers see only `{sub, name}`; they never learn credentials or
  contact info. Community-hosted servers are untrusted by design, so
  nothing sensitive may ever ride in the token.
- The service should store at most: credential material (Argon2id hash or
  passkey public key - prefer passkeys, nothing to leak), sub, display
  name, stats. No analytics, no third parties.

## Amendment: use an existing OIDC provider, not a custom issuer

Writing a credential-handling service from scratch is the wrong risk
trade-off when security is priority #1. Any OIDC provider that signs
with Ed25519 satisfies this ADR's wire contract (EdDSA JWT + JWKS).
**Rauthy** is the reference deployment: Rust, single container,
passkey-first, EdDSA by default, SQLite or Postgres, built for exactly
this kind of self-hosted HA setup. Keycloak or Zitadel also work
(both heavier; enable an Ed25519 signing key).

Parcello therefore only implements the verifier side:
`crates/server/src/eddsa.rs` - a JWKS refresh thread (multiple
`--identity-url`s, 15-min cadence, poked on unknown `kid`, a failed
fetch never drops known-good keys) feeding a cache that `verify` reads
without blocking. Claims: `sub`/`exp` required, display name from
`name`/`preferred_username`/`sub`, optional `aud` enforcement via
`--identity-audience`. Token identities are `id:<sub>`, non-spoofable
(no reconnect token needed to rejoin, ADR-0008).

## Consequences
- Game servers pass `--identity-url <jwks-url>` (repeatable, one per
  issuer instance) and optionally `--identity-audience <client-id>`.
- HS256 (ADR-0003) is deprecated (boot warning); removal one release
  after the EdDSA path has seen real use.
- Stats submission (server -> identity service) is out of scope here; it
  will need its own ADR - notably how an untrusted community server is
  prevented from forging stats.
