# Server summary (`crates/server`)

Sources of truth: crates/server sources, ADR-0003/0005/0008/0009/0010/
0014/0015/0016/0023/0028/0032/0034/0035, docs/INVARIANTS.md S*.

## Shape

axum; split lib (modules + `AppState` + `game_router`) / thin binary
(`main.rs`: flags + wiring only, anyhow at the boundary). The router
serves `/healthz`, `/config.json`, `/ws`, and the Flutter Web build
from disk (`--web-dir`, fails loudly without index.html).

## Transport (`ws.rs`)

Parse -> authenticate once (at create/join/spectate/queue) -> relay.
Caps: 64 KiB inbound message READ limit; per-connection token bucket
(burst 32, 16/s); global 1024-connection semaphore. `Session` binds the
verified identity to the connection; `relay` is an exhaustive match
(compile-time coverage of `ClientMessage`). Spectator sessions carry a
unique `watch:<n>:<id>` key and may only leave (acks dropped silently).
Mod ids from the wire are allowlist-validated (they become paths).

## Rooms (`room.rs` + `room/{clock,autoplay,tests}.rs`)

One Tokio task per room owns everything; `RoomCmd` messages in,
broadcasts out through per-connection unbounded senders. Phases
Lobby -> Active -> Finished (+ PlayAgain restart). Host = seat 0
(plain rooms). 2..=6 seats; bots (`is_bot`, `tx: None`) are played by
the room task via shared `bot::decide` at 800ms/move and yield seats to
joining humans. Rejoin by identity; spoofable (guest) seats require the
per-seat reconnect token (ADR-0008). Rooms dissolve after 30 min with
no connected seat AND no spectator; `Probe` messages deliberately do
NOT count as activity.

**Timers** (all in `Room::run`'s select, armed-state derived each
loop): AFK/turn clock (+ per-seat time bank, ADR-0023; jailed seats get
a 20s decision floor), absolute game clock (ADR-0010, never
animation-gated), bid window 12s / vote window 5s (auto-abstain
silent seats via ordinary injected commands), bot think, ranked lobby
grace 15s, showcase replay 10s, idle 30 min, animation-ack cap 10s.
The watermark (ADR-0028): every Update carries `seq`; animation-
sensitive timers wait for acks, bounded by the cap.

**Settings** (ADR-0015): `RoomSettings` = timers + full `RuleParams`;
host edits in lobby via `Configure`; every field clamped
(`clamp_settings` + `limits`); frozen at start (engine rebuilt with
effective rules -> replay-safe).

## Auth (`auth.rs`, `eddsa.rs`)

`IdentityVerifier` composite: EdDSA JWTs against JWKS
(`--identity-url`, background refresh, ADR-0009) > deprecated HS256
(`PARCELLO_JWT_SECRET`, ADR-0003) > guests (`--insecure-guest`,
spoofable). Identity = `player_id` (`id:`/`hs256:`/`guest:` prefix +
sub/name); display name is a sanitized cosmetic (ADR-0033, `@` handles
rejected). All broadcast text passes bidi/zero-width/control stripping.

## Persistence

`GameHistory` (ADR-0005): append-only port; SQLite adapter with a
dedicated writer thread (never blocks rooms); rows = complete replays.
`RatingStore` (ADR-0034): read-modify-write port; mutexed rusqlite in
the SAME file as history (WAL); reads via spawn_blocking; the one
at-game-end write in the room task is the documented exception.

## Ranked (`ranked/`) & showcase (`showcase.rs`)

See ranked-and-spectate-summary.md. Matchmaker task (2s tick) +
widening-window queue; showcase supervisor (15s tick) keeps one
all-bot room running only while no humans play (`--showcase`).

## LAN (`lan.rs`)

Opt-in UDP multicast announcer (ADR-0016), best-effort, no control
plane. Coverage-excluded; validated manually with the `discover` bin.
