# ADR-0037: Token lifecycle and transparent session recovery

Status: Accepted

## Context

ADR-0009 defined *how* an identity token is verified (EdDSA against the
issuer's JWKS). It never defined how long that token lives on the client,
or what happens when it stops being valid. The 2026-07 audit found that
nothing did:

- Both login flows read `id_token` out of the token response and threw
  the rest away - `refresh_token`, `access_token` and `expires_in` were
  never even parsed. Rauthy was returning all four (confirmed 2026-07);
  the refresh token was being delivered and discarded unread. **This is
  the whole defect: it was client-side, start to finish.**
- `oidc_login_io.dart` / `oidc_login_web.dart` also requested only
  `openid profile`. That was not what broke Rauthy - it grants refresh
  tokens without it - but it is what would break every issuer that gates
  them on `offline_access`.
- `GameSession._authToken` held that one string for the whole run of the
  app. There was no expiry tracking, no renewal, and no re-authentication.
- The server verifies `exp` on *every* auth-carrying message
  (`create` / `join` / `spectate` / `queue_ranked` / `get_rating`,
  `ws.rs::authenticate`). That is correct and stays.
- `GameSession._onClosed` dropped the player to the connect screen on any
  socket close, with no reconnect attempt - even though
  `docs/architecture.typ` section 6 specifies "WebSocket client with
  automatic reconnection and exponential backoff" for the Network Layer.

Consequence: at `exp` (the issuer's configured token lifetime - 1800s on
the reference deployment) the client's only credential became permanently
invalid, in memory, with no way to renew it. Every path back into a room
failed from that moment on. Since the client also had no reconnect at all,
a single transport blip after that instant ended the session - the player
was returned to the sign-in screen mid-game with their seat still held
open by the room they could no longer prove they owned.

The same defect is the reason a long menu session could no longer join a
second game: the second `join` re-verifies, and the token was dead.

## Decision

The client owns an explicit token lifecycle, and the socket recovers
itself. Three parts, all client-side; the wire protocol and the server's
"verify on every auth payload" rule are unchanged.

**1. Keep the whole grant** (and ask for `offline_access` so every issuer
grants one, not just the ones that do by default). Both login flows parse
the full token response into `OidcTokens` (`oidc_common.dart`). Expiry is read from the `id_token`'s
own `exp` claim - that claim is what the game server checks, so it is the
only authoritative deadline; `expires_in` describes the *access* token and
is a fallback, not the truth.

**2. `AuthManager` renews before expiry, and only then.** It is the single
source of the credential (`auth_manager.dart`):

- A timer rearmed on every new grant fires at `exp - RENEW_MARGIN` (120s),
  floored at 30s away: exactly one refresh per token lifetime, always
  ahead of the deadline.
- `freshIdToken()` is also checked lazily before every auth-carrying
  message and before every reconnect attempt, because timers do not fire
  while a laptop is suspended - a machine that wakes past `exp` must
  renew on the next use, not discover the failure at the server.
- Refreshes are single-flight: concurrent callers await the same request,
  so a burst of auth payloads costs one round trip.
- Refresh-token rotation is honoured (Rauthy rotates): the new
  `refresh_token` replaces the old one, and a grant that comes back
  without one keeps the previous.
- A refresh that fails with `invalid_grant` (revoked/consumed) is
  terminal: the manager reports "sign in again" rather than retrying a
  credential the issuer has rejected.

**3. Transparent reconnection with auto-rejoin.** `GameSession` remembers
the server URL and, on an *unexpected* close, reconnects with exponential
backoff (0.5s doubling to 15s, `_maxReconnectAttempts` = 8) instead of
falling back to the connect screen. When the socket is ready again, and a
room was in progress, it re-sends `join {code, auth}` with a freshly
minted token plus the seat's reconnect token. The server's existing
rejoin-by-identity path (ADR-0008) reattaches the seat and pushes the full
`Joined` snapshot, so recovery needs no user interaction and no new
message. Deliberate closes (`leave`, `disconnect from server`, dispose)
cancel reconnection - only a close the player did not ask for is retried.

**Security.** The refresh token is a long-lived bearer credential and is
held **in memory only**, never written to `reconnect.json` or
`localStorage`, never logged, and cleared when the player disconnects from
the server. This keeps the posture ADR-0009 chose for the id_token
(privacy over convenience) rather than trading it for cross-restart
convenience: a restarted client signs in again, exactly as before. The
client is public, so refreshes carry `client_id` and no secret, per
RFC 6749 section 6.

**Server.** `exp` verification gains a 60-second `CLOCK_SKEW_LEEWAY_SECS`
(`auth.rs`), applied by both the EdDSA and HS256 verifiers, so a token the
issuer still considers live is not refused over clock drift between two
separate machines. RFC 7519 section 4.1.4 sanctions a leeway and bounds it
("usually no more than a few minutes") but names no value; 60s is an
engineering judgement an order of magnitude inside that ceiling, and it is
not load-bearing now that the client renews 120s early. The full reasoning
- including why ~30s would be equally defensible and minutes would not -
is in the constant's doc comment, which is the single place to change it.

Note this renewal depends on the ID token being reissued on refresh, which
OIDC Core section 12.2 leaves optional. That dependency is a consequence of
authenticating with the ID token; ADR-0009 amendment 2 records why that
choice is kept and what would trigger revisiting it. The client detects a
refresh that returns no ID token (`OidcNoIdToken` -> `cannotRenew`) and
stops rather than retrying a call that cannot start succeeding.

## Consequences

- A signed-in player's session survives indefinitely while the app runs -
  token expiry is no longer an event the player can perceive.
- A dropped socket (proxy idle cut, Wi-Fi roam, laptop resume) restores
  the seat by itself. `docs/deployment.md`'s "reconnection reattaches the
  seat" is now something the client actually does rather than something
  the player has to perform by hand.
- One extra IdP round trip per token lifetime per signed-in client. On a
  1800s lifetime that is two requests an hour, and none at all for guests.
- **No deployment change is required.** The reference Rauthy already
  returned `access_token`, `id_token`, `refresh_token` and `expires_in`
  from the authorization code exchange, and reissues an ID token on
  refresh (verified 2026-07). The defect was entirely client-side: the
  refresh token was being handed to the client and thrown away unread.
  `offline_access` is requested for portability - other providers
  (Keycloak, Zitadel, Authentik) do gate refresh tokens on it - and costs
  nothing where it is already granted. An issuer that grants no refresh
  token still logs in fine; the session just ends at `exp`, as before.
- Guests are unaffected: they carry no token and nothing to refresh, and
  they reconnect through the same path.

## Alternatives considered

**Server-side session tokens.** `architecture.typ` section 5 sketches
"short-lived local session tokens for subsequent WebSocket frames". That
would decouple the game session from the IdP token entirely - but the
server already binds identity to the connection at join and never trusts
the wire identity again, so per-frame tokens would buy nothing here, and
minting a second credential class is a larger security surface than
renewing the one that exists. Rejected as unnecessary.

**Persist the refresh token.** Would let a restarted client resume without
signing in. Rejected: it writes a long-lived credential to plaintext
storage on disk (native) or `localStorage` (web, readable by any script
that gets injected into the origin), which is a real regression against
ADR-0009's memory-only stance, in exchange for convenience the reported
problem does not need.

**Let the server accept an expired token for a rejoin to a seat that
identity already holds.** Rejected outright: an expired token is not a
credential, and "you already had a seat" is not proof of anything an
attacker cannot also claim.
