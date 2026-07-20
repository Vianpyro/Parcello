# Extension guides

Step-by-step recipes for the changes people actually make, with every
file that must be touched, the mistakes already made once, and the
review checklist. Each recipe assumes you have read
`docs/INVARIANTS.md`; cited invariants (E2, P1...) live there.

The golden rule for all of them: **find the existing seam** (Strategy
traits, `ModPlugin`, `IdentityVerifier`, `GameHistory`/`RatingStore`,
`RuleParams`, the `RoomCmd` actor, the additive wire). If your feature
needs a NEW seam, that is an ADR, not a diff.

---

## 1. A new engine Command

Where: `crates/engine/src/command.rs` (variant), `apply.rs` pipeline
routing, an `Exec` method in the right `apply/` domain module
(movement/jail/trade/auction/estate/landing/cash/turn - all
`pub(super)`; the pipeline stays the only entry).

1. Add the `CommandKind` variant (snake_case wire name comes free from
   serde attributes - check the enum's attrs).
2. Validate FIRST, mutate the clone after (E2). Add a typed
   `CommandError` variant rather than overloading one ("Add a variant
   rather than overloading an existing one with a wrong message").
3. Emit events for everything that changed - events are the replay and
   the client's animation feed.
4. If the command is legal in a new situation, teach the fuzzer's
   generator (`next_valid_command`) - it panics on rejections.
5. If it can appear in `TurnPhase`-canonical play, extend
   `same_seed_produces_identical_games`'s action derivation.
6. Wire-format test in `crates/protocol` (P1).
7. CLI: `input.rs` parser + help text (`main.rs` doc comment + banner).
8. Flutter: `protocol.dart` (send helper) + whatever UI issues it.
9. Bot: does `bot::decide` need to know? (It must at least never emit
   an illegal command in the new state.)

Common mistakes: forgetting the fuzzer (panics in CI); forgetting the
CLI (it is the cheapest end-to-end check and CLAUDE.md requires it in
sync); validating against stale state after partial mutation.

Review checklist: rejection paths tested; events cover every mutation;
no cash movement inside an open window unless E6 is re-argued in an ADR.

## 2. A new engine Event

Where: `crates/engine/src/event.rs`, then the FAN-OUT - this is the
recipe people under-do:

1. Emit it from `Exec` (with seat indices, not names - views translate).
2. Wire test (engine serde tests or protocol tests).
3. Privacy: if it is party-scoped, extend `event_visible_to` AND
   `event_public` in `server/room.rs` (trade events are the model).
   Default is public - decide consciously.
4. CLI: `ui.rs::describe` arm (exhaustive match will force you).
5. Flutter: `protocol.dart::describeEvent` (localized line - BOTH ARB
   files) and, if it has a visual, a beat in `director.dart::_beatsFor`
   PLUS a budget test in `test/director_test.dart` (ADR-0030 makes the
   budget checkable - use it).

Common mistakes: adding the event but no `_beatsFor` beat (silent
invisible event); one ARB file only; leaking a private event to
spectators because `event_public` was forgotten.

## 3. A new rule scalar (mod-configurable number)

Decision first: is it GAME POLICY (fixed, engine-owned - lives in
`tuning.rs`: VP weights, mortgage percents, the 10% rebate) or a
MOD-CONFIGURABLE rule? Promoting a tuning constant to a rule REQUIRES
an ADR (CLAUDE.md). For a rule:

1. `RuleParams` field (engine `content.rs`) with an engine default that
   means "off" or "classic" where possible.
2. `GameContent::validate` if illegal values can break math (rent-table
   indexing taught us: `max_houses` caps at 5 because `rents[]` has 6
   levels).
3. `clamp_settings` + a named bound in room.rs's `limits` module (the
   wire value is untrusted host input - S3).
4. `mods/base/data/rules.toml` if the base mod sets it.
5. Settings UI: Flutter settings panel + CLI `set <field> <value>`.
6. Tests: engine behaviour at 0/edge values; a wire test if the
   `RoomSettings` shape grew (it does - `rules` is the full struct).

Common mistake: adding the field but not the clamp - every absurd-value
bug so far entered through an unclamped setting.

## 4. A new tile kind or card effect

Tiles: `content.rs::TileKind` + landing resolution in
`apply/landing.rs` + mod TOML schema + client rendering (board.dart
picks art by `kind`). Cards: `content.rs` card actions + the chain
executor (chains cap at depth 4 - keep it; it is the recursion bound).
Both: seeded randomness only through `GameState.rng` (E3); base-mod
content is A MOD (`mods/base/`), so ship the content change there, not
in engine defaults. Precedent to copy: The Audit (ADR-0029) for a tile,
the spotlight (ADR-0026) for state-carrying tiles - note WHERE its
state lives (GameState vs TileState) is a design decision with
transfer-semantics consequences; the ADR explains the choice.

## 5. A new protocol message

1. Variant on `ClientMessage`/`ServerMessage` in `crates/protocol` -
   additive shape only (P2): new optional fields get
   `#[serde(default, skip_serializing_if = ...)]`.
2. Wire-format test including the omitted-field/old-peer case.
3. `ws.rs`: connection-scoped messages get an arm in `handle_socket`
   AND a line in `relay`'s unreachable list; room-scoped ones get a
   `RoomCmd` + a `relay` arm (the exhaustive match is the net - P3).
   Decide explicitly what a SPECTATOR session may do with it (default:
   refused).
4. Room handler; remember replies to the offender only
   (`send_to`/`send_rejection`), broadcasts via `broadcast` (which now
   includes spectators - is your message public?).
5. CLI + Flutter handling (`ui.rs::render` and `session.dart::_handle`
   are both effectively exhaustive - follow the compiler/analyzer).
6. ws integration test over a real socket.

Precedents to copy: `list_mods` (connection-scoped query), `spectate`
(session-creating), `queue_ranked` (auth-carrying, state-owning).

## 6. A new room timer

`Room::run`'s select is the ONLY place time enters a room. Pattern
(copy `bid_deadline` or `showcase_replay`):

1. An `Option<Instant>` field (or a derivation method) = armed state.
2. In the loop: compute `armed` + a `sleep_until(deadline.unwrap_or(
   now + IDLE_TIMEOUT))`, guard the select arm with `if armed`.
3. Decide the ADR-0028 question explicitly: does this timer wait for
   the animation watermark (most do) or is it absolute (only the game
   clock is)? Document the answer at the field.
4. Firing must go through normal command paths (`handle_game` etc.) so
   replay integrity holds - never mutate state directly from a timer.
5. Paused-clock test in `room/tests.rs` (fires; and does not fire when
   disarmed).

Common mistakes: arming inside the select instead of deriving each
loop (mid-turn disconnects then don't shorten deadlines); counting a
non-activity message as activity (the Probe lesson, S6).

## 7. A new server flag / config.json field

Flag: `main.rs` Args (env alias `PARCELLO_*`), thread through
`build_state` into `AppState`, update the THREE AppState literals in
`tests/ws.rs` and any Room literals in `room/tests.rs` (the compiler
finds them), README flags paragraph, CLAUDE.md if behavioural.
Client-visible runtime default: add to `ClientConfig` (lib.rs) -
omit-when-unset for optionals, definitive booleans always serialized
(the `guest_allowed` precedent) - assert it in the config.json ws test,
read it in `connect_screen._probeServer`. NEVER put a secret there
(ADR-0032: public, unauthenticated surface).

## 8. A new persistence need

Use the port that matches the ACCESS PATTERN (S7): append-only
fire-and-forget -> extend `GameHistory` (new `Rec` variant, writer
thread, best-effort); read-modify-write -> `RatingStore` shape (mutexed
rusqlite, off-executor reads). A genuinely new pattern -> a new
Repository trait + memory & sqlite adapters + an ADR (that is the
architecture doc's own pattern, not a new seam). Schema changes:
`CREATE TABLE IF NOT EXISTS` migrations inline at `open()` - there is
no migration framework at this scale; if you introduce one, ADR first.
Same DB file is fine across ports (WAL, separate connections).

## 9. A new mod

`mods/<id>/mod.toml` + `data/*.toml`; last-loaded-wins per key
(tiles/cards replace by id, scalars override; conflicts log WARN);
`parcello_mods::resolve` then `GameContent::validate` gate it. Ids must
pass `valid_mod_id` (they travel the wire into paths). Test: resolve it
in a unit test; play it with bot CLIs. Boards render as any
`4*(d-1)` ring; other counts get the wrap layout. `mods/highroller` is
the rules-only example; `mods/base` the full one.

## 10. A new bot behaviour / AI opponent

`engine/src/bot.rs` is THE shared heuristic (server seats + CLI `--bot`
use the same `decide`, ADR-0014) - pure, deterministic given `noise`.
Extend `decide`'s decision list; add unit tests in `bot/tests.rs`; soak
with 3-4 bot CLIs. A genuinely different opponent personality = a new
pure function behind the same signature + an ADR for how it is selected
(there is no selection mechanism today - that absence is deliberate
simplicity, not an oversight).

## 11. A new Flutter screen / UI surface

Strings: ARB keys in BOTH files, never literals (C1). Colours/durations
from `tokens.dart`/`motion.dart` only (C2). Persistent in-game UI goes
in scrolling panels, not board overlays (C5) - and gets pumped by
`layout_test.dart` at all three sizes. Keyboard/controller: focusable
only when actionable, `FocusTraversalGroup` scoping, Escape = back
(`back_on_escape.dart`). Client-side persistence: the reconnect-token
store's reserved keys (`_issuer`, `_locale`, `_hints`) - the file
comment says to split a real prefs file when the next setting arrives;
that time is probably now (see technical-debt.md D8).

## 12. A new statistic or achievement

Read ADR-0009's stats note FIRST: an untrusted community server cannot
be allowed to write global stats - that is why ranked ratings are
per-server (ADR-0034). Per-server stats: follow the `RatingStore`
pattern (port + adapters + wire read message). Global anything:
requires the signed-results design that is explicitly deferred - do not
back into it via a "small" feature. Achievements are client-side
cosmetics until that exists; if stored server-side, they are per-server
facts and must say so in the UI.

## 13. A new replay-affecting anything

If your change alters what `(players, seed, commands)` replays to, it
is a REPLAY BREAK: old history rows stop replaying. That can be
acceptable (rules evolve) but must be called out in the ADR, and the
wire tests updated deliberately. Out-of-log steps are only tolerable if
pure and applied deterministically after the log (the `finish_on_time`
precedent, ADR-0010 - read its "Replay integrity" section before
inventing a second one).
