# Audit — debt H1: manual Dart mirror of the protocol (silent drift)

Status: **working document, not yet validated**. Nothing has been implemented.
Goal: lay out a complete state of play and argue for a strategy before any code
change. Originally drafted in French per the request; translated to English so
it can live in `docs/` (the rest of the tree is in English).

Scope inspected: `crates/protocol/src/lib.rs`, `crates/engine/src/{command,event,error,content,view,state}.rs`,
`crates/mods/src/{loader,manifest}.rs`, `clients/flutter/lib/{protocol,session,director,motion}.dart`
and every call site of `sendCmd(...)` under `clients/flutter/lib/ui/**`,
`crates/cli/src/*.rs` (as a comparison point), `.github/workflows/{ci,flutter}.yml`.

---

## 1. Executive summary

The network protocol (`ClientMessage`/`ServerMessage`/`CommandKind`/`Event`/
`CommandError`/all view structures) is defined **exactly once, in Rust**,
inside `crates/protocol` and `crates/engine`. The Rust CLI client
(`crates/cli`) imports these types directly — zero duplication, zero drift
risk, the compiler breaks on any divergence.

The Flutter client, on the other hand, cannot import Rust: it **hand-rewrites**
every one of these shapes in Dart. That is not a single duplication but a
cluster of at least **six independent duplication mechanisms** (detailed in
section 2), plus a **concrete CI gap** that turns this duplication into
*genuinely* silent drift: a Rust-side protocol change that doesn't touch
`clients/flutter/**` today triggers **no Flutter job at all**
(`flutter.yml` is filtered on that path; `ci.yml` explicitly ignores it in
return). A renamed `Event` variant, a field added to `ClientView`, a changed
timing constant: all of that can merge to `main`, green end to end, without a
single test ever having run it past the Flutter client's eyes.

The duplication is therefore not just a maintenance-comfort problem: it is a
**detection** problem. The core issue isn't "the same code is retyped twice",
it's "nothing ever says the two copies have diverged".

---

## 2. Full inventory of duplications

### 2.1 Client → server messages (`ClientMessage`, 17 variants)

Rust (`crates/protocol/src/lib.rs:62-150`) defines an externally-tagged enum
(`#[serde(tag = "type", rename_all = "snake_case")]`) with 17 variants:
`Create`, `Join`, `Spectate`, `AddBot`, `RemoveBot`, `Configure`, `Start`,
`PlayAgain`, `Leave`, `Cmd`, `Feedback`, `AnimationDone`, `ListMods`,
`QueueRanked`, `CancelQueue`, `GetRating`, `Ping`.

On the Dart side, **no mirror type exists at all**. `GameSession`
(session.dart) builds every message as a literal `Map<String, dynamic>`
encoded on the fly:

```dart
_ws?.sink.add(jsonEncode({'type': 'create', 'auth': _auth(''), if (mods.isNotEmpty) 'mods': mods}));
_ws?.sink.add(jsonEncode({'type': 'join', 'code': c, 'auth': _auth(c)}));
_ws?.sink.add(jsonEncode({'type': 'cmd', 'cmd': cmd}));
```

So this is a notch *below* a mirror: there isn't even a Dart class to keep in
sync, just literal strings (`'type'`, field names) repeated at every call
site. Nothing guarantees that `'type': 'add_bot'` actually matches the
`snake_case` tag serde derives from `AddBot` — the correspondence holds only
by convention and by test (`client_message_wire_format_is_stable`, Rust side
only).

### 2.2 Server → client messages (`ServerMessage`, 14 variants)

Symmetric case: Rust defines `RoomCreated`, `Joined`, `Spectating`, `Lobby`,
`GameStarted`, `Update`, `Rejected`, `Error`, `Mods`, `Queued`, `MatchFound`,
`Rating`, `RatingsUpdated`, `Pong`. Dart decodes them via a single `switch
(msg['type'])` in `GameSession._handle` (session.dart:416-511), with untyped
field-by-field access (`msg['code'] as String`, `msg['view'] as
Map<String, dynamic>?`).

Notable point: **this switch has no `default`**. An unknown message type
throws nothing, logs nothing — it silently falls on the floor. That is
exactly the mechanism that makes a new `ServerMessage` variant added
server-side invisible client-side until someone manually adds the matching
`case`.

Three messages (`Queued`, `MatchFound`, `Rating`/`RatingsUpdated`, ADR-0034)
are **not consumed by Flutter at all** — the project roadmap confirms this
("ranked menu greyed until a matchmaking service exists"). That's a latent
mirror surface: the day ranked is wired client-side, four new shapes will
need to be added to the mirror, with the same risk.

### 2.3 Game commands (`CommandKind`, 18 variants) — the most dangerous duplication

Rust (`crates/engine/src/command.rs`): `PlayMovementCard`, `Build`,
`ProposeTrade`, `AcceptTrade`, `DeclineTrade`, `CancelTrade`,
`SubmitBlindBid`, `SellHouse`, `Expropriate`, `BoostRent`, `Mortgage`,
`Unmortgage`, `ChooseLegalRoute`, `OfferBribe`, `VoteOnBribe`, `Resign`,
`EndTurn`, `UseJailCard`.

On the Dart side, these commands are built as `Map` literals **scattered
across six different UI files** (`ui/side/side_panel.dart`,
`ui/side/trade_dialog.dart`, `ui/game/game_screen.dart`,
`ui/game/actions_panel.dart`, `ui/game/nav_rail.dart`), each one retyping the
tag and field names:

```dart
s.sendCmd({'type': 'build', 'tile': def.id});
s.sendCmd({'type': 'submit_blind_bid', 'amount': (int.tryParse(_bid.text) ?? 0).clamp(0, cash)});
s.sendCmd({'type': 'choose_legal_route', 'order': _routeOrder});
```

This is the structurally riskiest duplication of the lot: there isn't even a
single gathering point (unlike 2.1, where everything at least funnels through
`session.dart`). A field rename (`tile` → `tile_id`, say) has to be found and
fixed across five independent widget files, with no Dart compiler ever
noticing — the error only surfaces at runtime, as a silently absorbed
`Rejected` or incorrect behavior.

### 2.4 Events (`Event`, 44 variants) — mirrored **twice** on the Dart side

This is the most interesting case: the same Rust enum is retranscribed
**twice independently** in Dart, for two different purposes, with two
different levels of coverage:

- `describeEvent()` (protocol.dart) — 44 `case`s covering roughly every
  variant, turns the event into a localized log line.
- `_beatsFor()` (director.dart) — only 29 `case`s, turns the event into an
  animation sequence (ADR-0030).

Both have **silent and different** fallback behavior:
- `describeEvent`: `default: return e.toString();` → an unhandled variant
  shows up as a raw Dart dump of the JSON `Map` in the log (ugly but
  visible).
- `_beatsFor`: `default: return const [];` (code comment: *"P4: never a
  beat"*) → an unhandled variant produces **no animation, no error, no
  log**. A new `Event` added on the Rust side and never wired into
  `director.dart` is a purely silent bug: the game keeps working, state stays
  correct, but nothing visually happens at the table for that specific
  event.

Two independent mirrors of the same enum, with two different coverage rates
(44 vs 29): this is the most concrete proof of "the protocol is defined
twice" taken in its most literal sense — here it's actually defined *three*
times counting Rust.

### 2.5 Rejection errors (`CommandError`, 35 variants)

Rust (`crates/engine/src/error.rs`): 35 typed variants, serialized as
`#[serde(tag = "code", rename_all = "snake_case")]`. Dart (`rejectReason()`,
protocol.dart): a 35-case switch translating each code to a localized
message. Fallback: `default: return code;` — an unrecognized code is shown
as-is (`"bid_below_floor"`) to the user instead of localized text. Silent
degradation, but a milder one (the player still sees something, just ugly
and untranslated) — comparable to D8 in `docs/technical-debt.md`, but never
formalized as a debt entry to date.

### 2.6 Shared data structures (16 types manually mirrored)

Each of these Dart classes carries a hand-written `.fromJson` constructor,
field by field, with unchecked casts (`j['x'] as int`, `as String?`):
`SeatInfo`, `TileDef`, `MarketEventDef`, `GameContent` (mirror of
`ResolvedContent`/`ModInfo`), `RuleParams`, `RoomSettings`, `PlayerView`,
`TileState`, `TurnPhase`, `TradeOffer`, `ScheduledEvent`, `ActiveMarketEvent`,
`MarketForecast`, `Spotlight`, `ClientView`, `RatingChange` (not consumed
today, see 2.2).

Every Rust field added, renamed, or retyped on `RuleParams`, `ClientView`,
`PlayerView`, etc. has to be manually propagated to its Dart counterpart. The
`j['field'] as int` cast doesn't silently "miss" an absent field, thanks to
the defensive `??` fallbacks already present everywhere — but that's a
discipline applied by hand at every line, not a structural guarantee.

### 2.7 Duplicated numeric constants

Cross-checked by grepping `crates/server/src/room.rs` against
`clients/flutter/lib/*.dart`:

| Rust constant | Value | Dart copy | File |
|---|---|---|---|
| `BID_WINDOW` (room.rs:62) | 12 s | `Duration(seconds: 12)` | session.dart:171 |
| `VOTE_WINDOW` (room.rs:67) | 5 s | `Duration(seconds: 5)` | session.dart:174 |
| `JAIL_DECISION_SECS` (room.rs:54) | 20 s | `_jailDecisionSecs = 20` | session.dart:137 |
| `ANIM_ACK_CAP` (room.rs:81) | 10 s | animation-tier budget (8s/6s/4s) | motion.dart, ADR-0030 |

These aren't protocol values in the strict sense (they never travel over the
wire), but they are **implicit timing contracts** between server and client:
the comment at `session.dart:167` says it itself — *"a local approximation of
the server's window [...] not a precise mirror"*. If the server changes
`BID_WINDOW`, nothing breaks at compile time or in tests; the client just
shows a visually wrong countdown.

On top of that, two formulas are already known and documented as debt **D8**
in `docs/technical-debt.md` (`GameState::net_worth` vs
`session.dart::netWorth()`, and `Exec::market_price` vs
`protocol.dart::marketPrice()`) — these are sub-cases of the same general
problem as H1, not separate debts to address in parallel.

### 2.8 Manual serialization/deserialization

Every `.fromJson` listed in 2.6, every `Map` hand-built in 2.1/2.3, and the
single encoding point `jsonEncode(...)` (no shared serialization layer)
together make up the entire Dart (de)serialization layer: entirely
hand-written, no generation, no schema validation on receipt. A wrong-typed
value sent by a server (bug, or a self-hosted third-party server slightly
ahead/behind on version) causes an uncaught Dart `TypeError` at the cast
site, not a clean deserialization error.

### 2.9 The CI gap that makes all of this silent

This is the missing piece that turns ordinary duplication into
**"silent drift"** debt, exactly as named in the problem statement:

```yaml
# .github/workflows/ci.yml (Rust)
paths-ignore:
  - "clients/flutter/**"   # ...among others

# .github/workflows/flutter.yml
paths:
  - "clients/flutter/**"   # sole trigger
```

A pull request that only modifies `crates/protocol/src/lib.rs` or
`crates/engine/src/{command,event,error}.rs` (adding a variant, renaming a
serde tag, adding a field) triggers `ci.yml` and **never**
`flutter.yml`. `flutter analyze`, `flutter test` and `flutter build web`
simply don't run. The PR can be green and merged without any tool having
even *attempted* to compile the Dart code that potentially references the
old tag.

This CI decoupling is a deliberate, documented choice (comment in
`flutter.yml`: *"Rust-only changes never pay the Flutter SDK setup"*) —
reasonable on its own for CI speed, but it leaves an exact blind spot over
the shared protocol. **Whichever strategy is chosen must close this gap**,
independently of whatever duplication mechanism is picked otherwise.

### 2.10 Numeric summary

| Category | Rust variants/fields | Dart mechanism | Dart files involved |
|---|---|---|---|
| `ClientMessage` | 17 | ad hoc `Map` literals, no type | session.dart + 5 UI files |
| `ServerMessage` | 14 | 1 switch with no `default` | session.dart |
| `CommandKind` | 18 | ad hoc `Map` literals, no type | 5 UI files |
| `Event` | 44 | 2 independent switches (44 and 29 cases) | protocol.dart, director.dart |
| `CommandError` | 35 | 1 switch, falls back to raw code | protocol.dart |
| View structures | 16 classes | 16 manual `.fromJson` | protocol.dart |
| Timing constants | 4 | copied-over values | session.dart, motion.dart |
| **CI** | — | disjoint Rust/Flutter trigger | `ci.yml` / `flutter.yml` |

---

## 3. Possible strategies

For each strategy: how it works, pros, cons, effort, risks, architecture
impact.

### S1 — Tooled discipline: "golden" conformance tests + closing the CI gap (no generation)

**How it works.** We don't touch the fact that the Dart mirror is
hand-written. We add a directory of versioned JSON fixtures
(`tests/protocol-fixtures/`, say — one canonical value per variant of the 5
enums). A Rust test verifies that `serde_json::to_string`/`from_str`
produces exactly these fixtures (extending what already exists in
`crates/protocol/src/lib.rs#tests`, generalized to *all* variants, not just
the ones covered today). A Dart test (`flutter test`) loads the same
fixtures and verifies that every `.fromJson`/switch handles them without
falling into a silent `default` (adding explicit "this type is handled"
assertions rather than letting the default case slip through). We fix
`ci.yml`/`flutter.yml` so that a change in `crates/protocol/**` or
`crates/engine/src/{command,event,error}.rs` also triggers `flutter.yml`
(or so that a lightweight job dedicated to the fixtures runs in both CIs,
without requiring the full Flutter SDK every time).

**Pros.** No new dependency, no generation tool, the CI blind spot closed
immediately. Makes drift *loud*: any forgotten variant fails a test in the
same PR that introduces it, on both the Rust and Dart sides. Very low entry
cost, understandable by any OSS contributor without learning a new tool.

**Cons.** Removes none of the six recorded duplications — it only makes them
detectable. The double-typing stays necessary on every change (adding an
`Event` variant still requires updating `describeEvent`, `_beatsFor`, and
now *also* the fixture — slightly more work, not less). The fixtures
themselves have to be kept up to date by hand; a rushed contributor might be
tempted to duplicate them instead of letting them fail properly.

**Effort.** Small to medium: a fixtures directory, an extension of the
existing Rust test, a Dart coverage test per switch, a fix to the CI trigger
paths.

**Risks.** Low — purely additive, no runtime change.

**Architecture impact.** None. Test/CI layer only.

### S2 — Lightweight generation of the Dart mirror from Rust types (intermediate schema + small in-house generator)

**How it works.** We derive `schemars::JsonSchema` on the protocol types that
are already `Serialize`/`Deserialize` (`ClientMessage`, `ServerMessage`,
`CommandKind`, `Event`, `CommandError`, `RuleParams`, `ClientView`, etc. — no
rewrite of their definitions, just one more derive). A small `xtask` binary
(pure Rust, in the workspace, no new external tool) calls `schema_for!` on
each type and emits a generated Dart file directly (`protocol.g.dart`:
`sealed class`/`fromJson`, plus a typed constructor per command variant — no
more scattered `Map` literals). The generated file is committed (same logic
as `gen-l10n`, already used in the project) and a CI check
(`cargo run -p xtask -- gen-dart --check`) fails if the committed file
diverges from what the current types regenerate. Since the generator is a
pure Rust binary, this check naturally runs in `ci.yml` (the Rust job),
without requiring the Flutter SDK — which closes the CI gap (2.9) *for
free*, without depending on the separate fix described in S1.

Research done: to my knowledge there is no mature, widely adopted equivalent
of `ts-rs`/`specta` targeting Dart (those tools exist for TypeScript). The
generator would therefore necessarily be a small in-house tool — but an
in-house tool built *on top of* an existing, reliable schema introspection
layer (schemars), not a hand-rolled Rust source parser.

**Pros.** Actually eliminates the structural duplication (2.1, 2.2, 2.3,
2.6): the Dart types stop existing independently, they *are* derived from
Rust. The Dart switches on `Event`/`CommandError` (2.4, 2.5) can become
exhaustive `switch`es on a generated `sealed class` instead of `switch`es on
`String` with a silent fallback — Dart 3 refuses to compile a non-exhaustive
switch on a `sealed class`: the single most dangerous point in this whole
audit (2.4, the invisible `default: return const [];` in `director.dart`)
becomes a **compilation error** instead of a silent production bug. This is
the only scenario that actually turns the "silent" in the debt's name into
"impossible to ignore".

**Cons.** Still a real piece of tooling to design and maintain (mapping
externally-tagged serde enums to idiomatic Dart `sealed class`es, handling
`#[serde(default)]`/`skip_serializing_if`, etc. — the non-trivial part isn't
schema extraction but the quality of the Dart emission). Adds a `schemars`
dependency to the workspace (to be checked under `cargo deny`/`cargo
machete`; MIT/Apache license, so a priori fine). The first migration is a
large PR (protocol.dart, session.dart, and the 6 UI files that build
commands as `Map`s) — not technically dangerous (the Dart compiler guides
the migration) but bulky to review. Does **not** remove the remaining
semantic duplication (the localized text in `describeEvent`, the
event-to-animation mapping in `director.dart` still have to exist by hand
somewhere) — only exhaustiveness becomes compiler-checked.

**Effort.** Medium to large once (designing + writing the generator,
migrating the existing code), then low ongoing (automatic regeneration on
every `cargo run -p xtask -- gen-dart`).

**Risks.** Medium: generation bugs on the subtlest serde cases (mixed-payload
enums, `Option` vs. absent field), possible regressions during the migration
of a large PR touching many UI files — mitigable by migrating type by type
rather than in one big bang.

**Architecture impact.** Adds a build step analogous to `gen-l10n` (an
already-accepted precedent in this project). Doesn't touch the wire format
(still `snake_case` JSON — no protocol ADR is called into question). The
`protocol` crate gains a dev/build dependency (`schemars`).

### S3 — Shared binary IDL (Protobuf / FlatBuffers) with official Rust + Dart generation

**How it works.** The canonical schema becomes a `.proto` file (or
equivalent); `prost` generates the Rust types, the official `protoc`
generator produces the Dart types; the WebSocket protocol moves from JSON to
a binary encoding.

**Pros.** Mature tooling on both sides (unlike S2, where the Dart side would
be homegrown), natively generated exhaustive enums in both languages, more
compact payloads.

**Cons — disqualifying here.** Calls into question a documented project
invariant: *"Wire-format tests exist; changing serde shapes is a protocol
break"* and *"the wire format IS the replay format"* (CLAUDE.md, protocol
section). The current format is also what makes the protocol inspectable/
debuggable directly in browser devtools during development of a hobby/OSS
project — a real asset lost with a binary format. Requires an extra external
toolchain (`protoc`, not just Cargo/Flutter) to pin in the devcontainer and
every CI — this is *the opposite* of the "reproducible builds" and "simple
architecture" priorities explicitly stated: it swaps a code-duplication
problem for a duplication problem *and* a third-party toolchain dependency.
Cross-cutting rewrite of the server (`ws.rs`), the CLI, and the client — an
undertaking out of all proportion to the actual size of the problem (small
JSON messages for a board game, not a high-performance system).

**Effort.** Very large.

**Risks.** High — cross-cutting regressions, a breaking change to a public
protocol that community servers may already expose (the "Minecraft" model
of independent self-hosted servers makes a protocol breaking change costly
socially, not just technically).

**Architecture impact.** Very large; contradicts several stated priorities.
Presented for completeness, but to be ruled out.

### S4 — Compile `protocol`/`engine` to WebAssembly and link Flutter against it (FFI/WASM), instead of mirroring it

**How it works.** `wasm-bindgen`/`wasm-pack` for web (`dart:js_interop`), and
`flutter_rust_bridge`/`cbindgen` + `dart:ffi` for desktop (Windows, Linux,
macOS) — two different binding mechanisms since WASM only covers the web.

**Pros.** The duplication disappears by construction: it's the actual Rust
code that also runs on the client, no second implementation.

**Cons — disqualifying here.** ADR-0025 deliberately chose *"a single Dart
codebase for desktop and web"* — a simple solution already in production.
This strategy splits it into two integration paths (WASM web-only + FFI
desktop-only), thus *increasing* architectural complexity instead of
reducing it, the exact opposite of the "simple architecture" priority. Even
if adopted, Flutter widgets would still need idiomatic Dart types
(`ClientView`, etc.) to bind against — we would very likely end up
rewriting a thin Dart layer around the WASM/FFI anyway, so the mirror
doesn't really disappear, it just moves and gets more complex. A heavy new
toolchain (two distinct binding chains) for a problem that fundamentally
only concerns (de)serializing small JSON messages.

**Effort.** Very large, two separate integration efforts.

**Risks.** High — a new class of bugs (FFI/WASM marshaling), crash surface
on platforms already flagged as fragile (the web OIDC flow is already listed
as "never exercised under real conditions" in the project's rough surfaces).

**Architecture impact.** Very large; undoes a recent, well-functioning ADR
decision for marginal benefit over S2. Presented for completeness, but to be
ruled out.

---

## 4. Comparison table

| | S1 — golden tests + CI fix | S2 — lightweight generation (schemars + xtask) | S3 — binary IDL | S4 — WASM/FFI |
|---|---|---|---|---|
| Single source of truth | No (duplication detected, not removed) | Yes (Dart types derived from Rust) | Yes | Yes |
| Code generation | None | Small, in-house, targeted | Heavy, external tooling | Very heavy, two toolchains |
| Architecture simplicity | Unchanged | +1 build step (like gen-l10n) | Large change | Large change, splits desktop/web |
| Reproducible build | Unchanged | Yes (Cargo only) | New external binary required | Two extra native toolchains |
| OSS maintenance | Easy, nothing new to learn | Moderate (understanding the generator) | Hard (protoc, breaking wire) | Hard (FFI/WASM) |
| Closes the CI gap (2.9) | Yes, explicitly | Yes, as a side effect | Yes | Yes |
| Makes "silent" compilable | No (just tested) | Yes (exhaustive `sealed class`) | Yes | Yes |
| Effort | Small-medium | Medium-large then small | Very large | Very large |
| Risk | Low | Medium | High | High |
| Consistent with stated priorities | Yes | Yes | No | No |

---

## 5. Recommendation

**Two-phase recommendation, not a binary choice: S1 then S2.**

**Phase 1 (immediate): S1.** Add the golden fixtures and fix the CI
trigger. It's cheap, requires no committing architecture decision, and
*immediately* closes the most severe gap in the audit (2.9): the fact that a
Rust protocol change can merge today without any Dart tool ever having seen
it. Even if S2 is adopted afterward, this work isn't wasted: the fixtures
become the generator's test vectors.

**Phase 2 (to be scheduled): S2.** This is the only strategy that actually
meets the stated goal — *"a single source of truth"* — without sacrificing
any of the listed priorities (simple architecture, minimal generation,
reproducible builds with Cargo alone, easy maintenance for an OSS project).
The decisive point in its favor, beyond removing the duplication: it turns
the worst case found in this audit — the silent `default: return const
[];` in `director.dart` (2.4), where an unhandled server event breaks
nothing and shows nothing — into a Dart compilation error via `sealed
class` exhaustiveness. That's the difference between "the protocol is
tested" (S1) and "the protocol cannot diverge without `flutter analyze`
refusing to compile" (S2), which is the closest formulation to "single
source of truth" achievable without breaking the JSON format or splitting
the current desktop+web client architecture.

S3 and S4 are ruled out: both solve the problem in a seemingly "cleaner" way
(a single real codebase executed, or a mature IDL on both sides), but at the
cost of an architecture change wildly disproportionate to the actual size of
the problem — a few dozen JSON types for a board game — and directly
contradicting at least two explicitly stated priorities ("simple
architecture", "minimal code generation").

**Why not do S2 directly, without S1?** Because S2 is a non-trivial project
(designing the generator, migrating six UI files) that will take time before
it can merge, during which the CI gap (2.9) stays open. S1 is mergeable as a
small PR and closes that gap today, independent of S2's timeline.

---

## 6. What I did *not* do

Per the request, no code has been modified. This document has also not been
added to `docs/AI_ENGINEERING.md` nor referenced from
`docs/technical-debt.md` — to be done once the strategy is validated (and,
originally, once the document was translated to English if it were to live
in `docs/` long-term — now done).
