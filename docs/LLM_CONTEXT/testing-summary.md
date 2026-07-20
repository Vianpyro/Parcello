# Testing summary

Source of truth: docs/testing.md (philosophy + full map), CLAUDE.md
(CI numbers).

## The three named contracts (never weaken)

1. `same_seed_produces_identical_games` (engine) - the replay
   contract; extend its canonical actions when adding a `TurnPhase`.
2. Wire-format tests (`crates/protocol`) - compatibility contracts;
   exact-JSON including omitted-field/old-peer cases.
3. `director_test.dart` - the client animation budget vs the server's
   10s ack cap (ADR-0030).

## Where tests live

engine rules -> `crates/engine/tests/` themed files over shared
fixtures | legality -> also the fuzzer's generator (it PANICS on
rejected generated commands and asserts money/state invariants every
step) | room lifecycle/timers -> `room/tests.rs` (paused tokio clocks)
| connection flows -> `crates/server/tests/ws.rs` (REAL server, real
sockets, `recv_until`) | validators/adapters -> unit tests beside them
| rating math/queue policy -> pure unit tests | Flutter ->
`clients/flutter/test/` (protocol, director budget, stage render, bid
input, LAYOUT at 1280x800/1280x720/1024x600 where a pumped overflow is
a failure, spectate+hints, oidc native).

## Musts

Every rule: accept + reject + events. Every serde shape (+ old-peer
case). Every new ClientMessage: happy path AND refusal over a real
socket. Every validator: hostile inputs. Every room timer: fires +
doesn't when disarmed. Every l10n key: BOTH ARB files.

## Deliberately untested (don't "fix")

`crates/cli` (it IS the harness), `server/main.rs` (wiring),
`lan.rs` (manual `discover` bin) - all coverage-excluded. Pixel art.
The web OIDC popup vs a real IdP (manual QA item, debt D2).

## CI gates (all must pass locally before claiming done)

fmt, typos, clippy pedantic+nursery `-D warnings`, `cargo test
--workspace --locked`, llvm-cov line coverage >= 88% (ratchet; cli/,
main.rs, lan.rs excluded), MSRV 1.96 check, rustdoc `-D warnings`,
machete + deny; Flutter job (analyze/test/web build) when
clients/flutter changes.

## Cheap end-to-end soaks

3-4 `--bot` CLIs to a full game (turn-flow changes); `--ranked` + two
`--queue --bot` token CLIs (match at 60s fallback -> ratings rows);
`--showcase` + `--spectate` (attach within one 15s tick); headless-
browser screenshot of the served web bundle.
