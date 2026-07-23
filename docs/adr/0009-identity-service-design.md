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

## Amendment 2 (2026-07): the ID token is the credential, deliberately

This ADR said "EdDSA JWT" and never chose between OIDC's two tokens. The
client picked `id_token` in `oidc_login_io.dart` and the deployment guide
documented it afterwards, so the decision was *inherited from an
implementation*, not made. Reviewed and now made explicitly.

**By the specifications, this is the unconventional choice.** An ID token
is an authentication assertion addressed to the *client*: OIDC Core
section 2 requires its `aud` to be the client_id, and section 3.1.3.7
requires the client to check exactly that. The credential intended for a
separate party is the access token (RFC 6749), with RFC 9068 defining its
JWT profile (`typ: at+jwt`, resource-named `aud`, a `scope` claim). Every
major IdP vendor publishes "do not use ID tokens to call APIs". The game
server is a separate party from the Flutter client, so by the book it is a
resource server and should receive an access token.

**Parcello keeps the ID token anyway,** because the property this ADR is
built on is *stateless offline verification with no per-login IdP call*,
and only the ID token guarantees it across "any OIDC provider":

- **ID tokens are always JWTs.** Access tokens are format-unspecified;
  several major IdPs issue opaque ones by default, verifiable only by
  introspection (RFC 7662) - a network call per login. That would break
  "servers never call the service per login", and with it the redundancy
  argument above ("already-issued tokens keep working even if every
  instance is down").
- **ID tokens carry the profile claims we actually consume.** `name` /
  `preferred_username` feed `safe_display_name`. RFC 9068 makes those
  claims optional in an access token, so the alternative is a `/userinfo`
  round trip - the same per-login call, again.
- **The security delta is ~zero here.** The standard objections do not
  bite: Parcello has no scopes and exactly one permission level, so an
  access token's `scope` claim would encode nothing; and the audience
  objection is answered by `--identity-audience`, which is now warned
  about at boot when unset (see Consequences).
- **It fixes nothing real.** The genuine residual risk is a malicious
  community server replaying a player's token to *another* Parcello
  server. Both token types share `aud` across all community servers, so
  switching does not help; that needs per-server audiences or
  proof-of-possession (DPoP), which is its own ADR.

**Accepted cost, and it is real:** OIDC Core section 12.2 makes `id_token`
OPTIONAL in a refresh response. An issuer that declines to reissue one
cannot renew a Parcello session at all (ADR-0037). Rauthy does reissue,
so the reference deployment is fine; the client detects the case
explicitly (`OidcNoIdToken` -> `AuthManager.cannotRenew`) and asks for a
fresh sign-in instead of retrying forever.

**Revisit if** any of these become true: Parcello needs scopes or
differentiated permissions; a target IdP will not reissue ID tokens on
refresh; or per-server audiences / DPoP are taken on - at which point
access tokens become the right shape and this should be reopened.

## Consequences
- Game servers pass `--identity-url <jwks-url>` (repeatable, one per
  issuer instance) and optionally `--identity-audience <client-id>`.
- **`--identity-audience` should be considered required in any real
  deployment**, and its absence is warned about at boot (main.rs). With
  an ID token the `aud` claim is the only thing asserting the token was
  minted for Parcello; without the check the server accepts every token
  its issuer signs, including ones minted for an unrelated application
  sharing that issuer.
- HS256 (ADR-0003) is deprecated (boot warning); removal one release
  after the EdDSA path has seen real use.
- Stats submission (server -> identity service) is out of scope here; it
  will need its own ADR - notably how an untrusted community server is
  prevented from forging stats.
