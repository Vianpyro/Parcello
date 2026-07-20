# Testing

How this repo decides what to test, where a new test belongs, and what
is deliberately untested. The CI gate numbers live in CLAUDE.md
("Development & testing" in README has the user-facing view); this file
is the philosophy and the map.

## Philosophy

1. **Test behaviour at the smallest boundary that owns it.** Engine
   rules get engine tests (no server, no sockets). Wire shapes get
   protocol tests. Connection state-machine behaviour gets real-socket
   integration tests. Rendering/layout gets pumped widget tests. Do not
   test an engine rule through the WebSocket - when it breaks you want
   the failing test to point at the layer that owns the bug.
2. **Contracts get named guards.** The three load-bearing ones:
   - `same_seed_produces_identical_games` - the replay contract
     (ADR-0001/0002). Extending `TurnPhase` REQUIRES extending its
     canonical-action derivation.
   - the wire-format tests in `crates/protocol` - the compatibility
     contract (P1/P2). A serde-shape change that doesn't touch them is
     a break that CI cannot see.
   - `test/director_test.dart` - the animation budget (ADR-0030)
     against the server's `ANIM_ACK_CAP`.
3. **A pumped frame is an assertion.** `test/layout_test.dart` renders
   the LOADED game screen (six trades, six seats, running clock) at
   1280x800 / 1280x720 / 1024x600; a `RenderFlex overflowed` fails the
   test. New persistent UI must participate in these pumps (scrolling
   panels, not floating overlays - invariant C5).
4. **The fuzzer is an invariant machine, not a bug lottery.**
   `game_state_fuzzer.rs` generates ONLY commands it believes legal,
   panics if the engine rejects one (generator and engine must agree on
   legality), and asserts money conservation + state invariants after
   every step. When you change a rule, the generator is part of the
   change.
5. **Coverage is a ratchet, not a target.** CI enforces
   `COVERAGE_MIN_LINES` = 88 with `cli/`, `server/main.rs`, `lan.rs`
   excluded. Keep the floor 2-3 points under measured (currently ~90)
   so it catches collapses, not refactors. Never write a test whose
   only purpose is coverage.

## The test map (where does my test go?)

| What changed | Test lives in | Style |
|---|---|---|
| Engine rule / command / event | `crates/engine/tests/` themed files (`engine.rs` core+wins, `auction_and_trade.rs`, `jail_and_corruption.rs`, `estate_and_economy.rs`) over `tests/common/mod.rs` fixtures (`plain_board`...) | scripted `step`/`play` sequences asserting events + state |
| Anything touching legality | also: fuzzer generator (`game_state_fuzzer.rs`) | keep generator/engine agreement |
| Serde shape (command, event, message) | `crates/protocol/src/lib.rs` + engine wire tests | exact-JSON assertions incl. omitted-field compat |
| Room lifecycle, timers, bots, autoplay | `crates/server/src/room/tests.rs` | tokio `test-util` paused clocks; Room built literally |
| Connection state machine, auth-once, relay, spectate/ranked flows | `crates/server/tests/ws.rs` | REAL axum server on an ephemeral port, real websockets, `recv_until` |
| Validators (rate limit, mod ids, sanitizers) | unit tests next to the validator | hostile-input tables |
| Persistence adapters | unit tests in `history.rs` / `ranked/store.rs` | tempfile SQLite, reopen-and-assert |
| Rating math / placements / queue policy | `ranked/ladder/tests.rs`, `ranked/queue.rs` tests | pure functions, no clock |
| Flutter protocol/render/session | `clients/flutter/test/` (`protocol_test`, `director_test`, `stage_render_test`, `bid_input_test`, `layout_test`, `spectate_and_hints_test`, `oidc_test`) | widget pumps + pure unit tests |

## What MUST be tested (non-negotiable)

- Every non-trivial engine rule: the accepting path, at least one
  rejecting path, and the emitted events (they are the replay).
- Every new serde shape, including the "old peer omits the field" case.
- Every new `ClientMessage`: its happy path over a real socket AND its
  refusal path (wrong session kind, wrong phase, wrong actor).
- Every untrusted-input validator with hostile inputs (traversal
  attempts, bidi characters, overlong, negative).
- Every timer you add to `Room::run`: a paused-clock test that it fires,
  and one that it does NOT fire when disarmed.
- Both ARB files for every new l10n key (gen-l10n only fails loudly for
  the template; the FR file drifts silently otherwise).

## What may stay untested (and why)

- `crates/cli` - it IS a test harness; its value is exercising the
  server by hand. (Coverage-excluded.)
- `server/main.rs` - flag parsing and wiring; `build_state` is thin and
  the router it wires is fully integration-tested. (Excluded.)
- `lan.rs` - best-effort UDP announcing; the `discover` bin exists for
  manual validation (ADR-0016). (Excluded.)
- Pixel-exact rendering - layout tests catch overflow; art direction is
  reviewed by eyes, not asserted.
- The web OIDC popup flow against a real IdP - compilation-checked
  only; listed as a rough surface in CLAUDE.md; needs one manual QA
  pass per release until someone automates a browser harness.

## Soak & end-to-end recipes (manual, cheap, high-yield)

- **Bot table**: `--insecure-guest` server + 3-4 `parcello-cli --bot`
  seats; run to completion when touching turn flow (CLAUDE.md asks for
  this explicitly).
- **Ranked loop**: server `--ranked` + HS256 secret; two `--queue --bot`
  CLIs with minted tokens; expect match at the 60s fallback, auto-start,
  `ratings_updated`, rows in `rating`/`rated_game`.
- **Spectate/showcase**: server `--showcase`; `parcello-cli --spectate`
  should attach to the bots game within one 15s supervisor tick.
- **Web smoke**: serve the built bundle (`--web-dir`), open `/`, check
  the reachability line and (against a no-guest server) the disabled
  guest path. Headless chromium screenshots of `/` are enough to catch
  a broken bundle.

## Known testing gaps / opportunities

- Engine purity (E1) has no mechanized check - candidate: deny ban-list.
- `Probe`-is-not-activity (S6) and room idle dissolution lack a
  paused-clock regression test.
- Property-based testing beyond the fuzzer (e.g. proptest on trade
  re-validation, mortgage arithmetic) would pay off; the fixtures make
  it cheap. Nobody has needed it yet.
- Performance tests: none, deliberately - see docs/performance.md for
  why benches don't exist and what would justify the first one.
- Ranked queue fairness under churn (many joins/leaves) is unit-tested
  at the `propose_match` level only; a stochastic soak harness would
  catch policy regressions if matchmaking ever gets tuned.
