# Security model

The threat model for a game whose servers are COMMUNITY-HOSTED. This is
the document to re-read before touching auth, the wire, or anything that
crosses a trust boundary. Mitigation locations are named so a reviewer
can check they still exist.

## Trust boundaries (the map)

1. **Client -> game server**: every byte is untrusted. Clients are
   renderers; the server validates everything (architecture.typ's
   authoritative-server goal).
2. **Game server -> players**: the server is untrusted TOO. A community
   host can read every hidden bid, rig seeds, or lie about results.
   This is accepted by design (Minecraft model): the mitigation is
   social (you pick whose server you join) and structural (nothing
   sensitive ever rides in tokens - ADR-0009; ladders are per-server so
   a cheating host only corrupts their own reputation - ADR-0034).
   **Never** build a feature that requires trusting a community server
   (a global ladder needs signed results first - ADR-0009's stats note).
3. **Game server -> identity provider**: public keys only (JWKS,
   EdDSA). Servers never hold secrets, never call the IdP per login
   (ADR-0009). The deprecated HS256 path is the one exception (shared
   secret) and is why it must eventually die (ADR-0003).
4. **Operator -> server**: flags/env are trusted; mod bundles on disk
   are trusted CONTENT but still validated as GAME DATA
   (`GameContent::validate`) so a bad mod fails loudly, not weirdly.
5. **Web page -> server**: `/config.json` is public and unauthenticated
   BY DESIGN - only ever put values there that any visitor may see
   (ADR-0032). It is a config surface, not a secrets channel.

## Attack surfaces and standing mitigations

| Surface | Attack | Mitigation (location) |
|---|---|---|
| WS frames | oversized allocation pre-parse | `MAX_WS_MESSAGE_BYTES` = 64 KiB read cap (ws.rs) - never a write cap |
| WS message flood | CPU/queue exhaustion | token bucket `MSG_BURST`=32 / `MSG_REFILL_PER_SEC`=16 per connection (ws.rs) |
| Socket flood | fd/memory exhaustion | `MAX_CONNECTIONS` = 1024 semaphore (lib.rs); per-IP throttling DELEGATED to the reverse proxy (docs/deployment.md) |
| Spectator fan-out | amplification via watchers | `MAX_SPECTATORS` = 32/room; dead senders pruned; spectate requires auth like join (ADR-0035) |
| Mod ids on Create | path traversal (ids become paths) | `valid_mod_id` allowlist charset, max 16 ids (ws.rs) |
| Room settings | absurd values breaking the engine | `clamp_settings` + `limits` module (room.rs); engine re-validates at start |
| Names/comments | terminal escapes, bidi/zero-width spoofing, log injection, email leakage | `sanitize_guest_name`, `sanitize_display_name` (rejects `@`), `sanitize_comment` + `is_unsafe_format` (auth.rs/room.rs); parameterized SQL everywhere |
| Guest identity | first-join squatting; mid-game seat theft | mid-game: per-seat reconnect tokens, constant-time compare (ADR-0008). First-join squatting is a DOCUMENTED residual (below) |
| Token replay | stolen JWT reuse | `exp` enforced on EVERY auth-carrying message, with a 60s clock-skew leeway (`auth::is_live`, ADR-0037); out of scope beyond that (bearer-token model, ADR-0009) |
| Wrong-audience token | a token minted for another app on the same issuer | `--identity-audience` checks `aud`; REQUIRED in practice because the credential is an OIDC ID token, whose `aud` is the only "meant for Parcello" assertion (ADR-0009 amendment 2). Warned at boot when unset |
| Refresh token | long-lived credential theft | memory-only on the client - never `reconnect.json`, never `localStorage`, never logged; dropped on disconnect (ADR-0037). Redeemed over TLS with `client_id` only (public client, no secret) |
| Ranked queue | rating on a forgeable identity | spoofable identities refused at queue AND `get_rating` (ADR-0034) |
| Ranked queue | orphaned entries ghost-matching | re-queue purges the connection's previous entry; removal is connection-scoped (`same_channel`) |
| Sealed bids | seeing others' pending bids | masked in `for_seat`/`for_spectator` views; amount-less `BlindBidSubmitted` event (E5) |
| RNG | predicting draws | seed/deck never serialized into any view (E4) |
| Idle resources | rooms living forever | 30-min idle dissolution; `Probe` explicitly does not count as activity (S6) |
| History DB | SQL injection via comments | parameterized statements only (history.rs, store.rs) |

## Cheating analysis

- **Server-side cheating**: possible by construction on boundary 2;
  accepted. What limits blast radius: per-server ladders, no PII in
  tokens, replay logs a host could publish for audit (nothing forces
  them to - see recommendations).
- **Client-side cheating**: nothing to cheat WITH - clients hold no
  authority. The interesting surface is information (fixed: views are
  masked server-side) and timing (fixed: server timers, `ANIM_ACK_CAP`
  bounds stalling).
- **Collusion / boosting in ranked**: two accounts feeding wins is
  undetectable server-side today. Accepted at current scale; the
  per-server ladder means the operator can just reset ratings. Revisit
  if a global ladder ever lands.
- **Smurfing**: new accounts start at 1000; nothing prevents it. The
  identity provider is the place for friction (one account per person),
  not the game server.
- **AFK griefing**: bounded - AFK seats are auto-played the canonical
  action, never spend cash (S5), and windows auto-abstain. A griefer's
  ceiling is being a boring opponent.

## Residual risks (accepted, with reasoning)

1. **HS256 stopgap** (ADR-0003): shared-secret auth survives until
   LAN/WAN playtests finish (owner decision 2026-07). Risk: any server
   holding the secret can mint tokens for any `sub` in that trust
   domain. Do not build anything new on it; removal criteria in
   docs/technical-debt.md.
2. **Guest first-join squatting**: anyone can take any free name on an
   `--insecure-guest` server. Documented limitation; real fix is
   accounts, which exist. The flag's own name and the boot warning are
   the guardrails.
3. **No per-IP limits in-process**: delegated to the deployment's
   reverse proxy on purpose (keep the binary simple; every serious
   deployment has one - docs/deployment.md). A bare internet-exposed
   binary without a proxy accepts this risk.
4. **Room-creation churn**: creating rooms is cheap for an attacker
   with a connection (each holds a Tokio task + content Arc for up to
   30 min idle). Bounded by `MAX_CONNECTIONS` and per-connection rate
   limits; a determined attacker could still hold ~1024 rooms. Accepted
   at hobby scale; see recommendations.
5. **CLI token on the command line**: `--token` is visible in process
   lists. Test-harness tool; acceptable. Do not copy the pattern to
   anything user-facing.
6. **Showcase CPU**: `--showcase` burns one bot game (~1 command/800ms)
   indefinitely on an empty server. Opt-in; trivial load; documented.
7. **Refresh token in client memory** (ADR-0037): renewing a session
   means holding a long-lived credential for the run of the app. It is
   never persisted, so it dies with the process and cannot be stolen
   from disk - but anything that can read the client's memory (or
   inject script into the web origin) can read it, exactly as it could
   already read the id_token. Accepted: the alternative was a session
   that ends mid-game every token lifetime.
8. **A malicious community server can replay your token elsewhere**
   (ADR-0009 amendment 2): every Parcello server shares one issuer and
   one `aud`, so a server you join receives a bearer credential that is
   equally valid at any other server, until `exp`. This is inherent to
   the bearer model and is NOT improved by switching to access tokens -
   they would carry the same audience. The real fixes are per-server
   audiences or proof-of-possession (DPoP), both their own ADR. Bounded
   today by short token lifetimes and by the fact that a token carries
   no PII and grants nothing outside Parcello (ADR-0009 privacy). This
   is the strongest argument for keeping the issuer's token lifetime
   short - which ADR-0037's renewal now makes free.

## Review rules for security-relevant changes

- New wire field or message: does anything in it reach a filesystem
  path, SQL string, terminal, or broadcast text? Then it needs a named
  validator at the boundary (S3) and a test proving the hostile case.
- New view field: could it be derived from `rng`/deck order or another
  seat's hidden state? (E4/E5.)
- New timer: is it animation-gated, and should it be? (S4 - the game
  clock precedent says absolute deadlines never gate.)
- New identity-adjacent feature: does it key on `player_id` and refuse
  spoofable identities where persistence is involved? (S2.)
- Anything "the server will trust the client briefly": no.

## Recommendations (future, in rough priority order)

1. Finish the EdDSA migration and delete HS256 one release after real
   playtests (tracked in technical-debt.md).
2. Optional replay publication: a server flag to expose finished-game
   logs (seed + commands are already stored). Cheap transparency lever
   for boundary 2; needs an ADR (privacy of guest names in logs).
3. Signed match results, IF a global ladder is ever wanted (big: an
   identity-service extension; see ADR-0009 consequences and ADR-0034's
   deferral).
4. Room-creation token bucket per connection if churn abuse is ever
   observed in the wild (one counter next to the message limiter).
5. `cargo deny` ban-list for `crates/engine` dependencies to mechanize
   invariant E1.
