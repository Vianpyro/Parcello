# AI engineering handbook

The entry point for any AI (or new human) working on Parcello. This file
tells you how to WORK here; it does not repeat what things are - it
points at the document that owns each topic. Assume your context window
is smaller than the repo: read in the order below, load only what the
task needs, and trust the pointers.

## Reading order (per task, not upfront)

1. `CLAUDE.md` - always. Hard constraints, repo map, commands, rules
   snapshot. It is dense on purpose; it is the index.
2. `docs/INVARIANTS.md` - before ANY code change. Audit your plan
   against it; a violated entry means your plan needs an ADR first.
3. The ADRs your task touches (CLAUDE.md's ADR list maps topics to
   numbers). `docs/architecture.typ` is the constitution, READ AS
   AMENDED by the ADRs - where they disagree, the ADR is newer and
   wins (that is the amendment system working, not a bug; see
   technical-debt.md D4).
4. `docs/LLM_CONTEXT/<subsystem>-summary.md` - when you need a
   subsystem you are NOT changing (cheap orientation).
5. `docs/extension-guides.md` - if your task matches a recipe, follow
   it; the checklists encode mistakes already made once.
6. Domain questions -> `docs/domain-model.md` +
   `docs/business-tour-direction.md` (the design intent).
   Feel/animation -> `docs/motion-language.md` + ADR-0028/0030.
   Trust questions -> `docs/security-model.md`.

## The decision process

- **Bug fix / additive feature behind an existing seam**: just do it,
  with tests, following the recipe.
- **Anything contradicting architecture.typ, an ADR, or INVARIANTS**:
  write the ADR FIRST (context / decision / consequences; alternatives
  when they were genuinely weighed - see the ADR guide below), get it
  agreed, then code. The codebase's whole history follows this shape.
- **New seam** (trait, port, service, protocol concept): that is an
  architecture change even if the diff is small. ADR first. The
  sanctioned seams are: Strategy traits in the engine, `ModPlugin`,
  `IdentityVerifier`, `GameHistory`/`RatingStore` (Repository pattern -
  a NEW repository trait for a new access pattern is the pattern
  working, not a new seam), `RuleParams`, the `RoomCmd` actor boundary,
  and the additive wire.
- **Unsure which**: it needs an ADR. Ambiguity about whether something
  is architectural IS the signal.

## How to write an ADR here

File: `docs/adr/00NN-kebab-title.md`. Shape (match the house style -
read 0031 for a small one, 0034 for a large one):
- **Context**: the forces, including what playtests/operations showed.
  Cite the documents you are deviating from BY NAME.
- **Decision**: what, precisely, with the key parameters. If you
  weighed real alternatives, say why the losers lost (0034's
  Elo/Glicko/TrueSkill paragraph is the model) - but never invent
  alternatives you didn't weigh; false history is worse than none.
- **Consequences**: costs accepted, wire/replay impact, what is
  deliberately deferred, what future work it unlocks or blocks.
- **Amendments**: append dated amendment sections to EXISTING ADRs
  rather than rewriting them (0018 has two; the trail is the value).
Then update: CLAUDE.md's ADR list, README's deviations list,
INVARIANTS.md if a new invariant was created, and the relevant
LLM_CONTEXT summary. An ADR that ships without those updates is
incomplete.

## Review methodology (what "reviewed" means here)

Run this list against any non-trivial diff - it is the distilled form
of the reviews that caught real bugs in 2026-07:

1. **Invariant audit**: walk INVARIANTS.md sections touching your
   layers. Especially: E2 (mutation before validation), E5/E4 (view
   leaks), P1/P2 (serde shapes), S3 (unvalidated wire input), S4
   (timer gating), C4 (Dart null-seat traps).
2. **Fan-out completeness**: engine change -> fuzzer generator, wire
   tests, CLI, Flutter, bot, describeEvent/beats, BOTH ARB files. The
   recipes list the exact chains.
3. **Test the refusal, not just the success.**
4. **Struct-literal blast radius**: `AppState` and `Room` are built
   literally in tests; field additions touch tests/ws.rs (3 sites) and
   room/tests.rs (3 sites). Let the compiler enumerate them.
5. **Gates locally before claiming done**: `cargo fmt`, `clippy
   --workspace --all-targets --locked -- -D warnings` (pedantic+nursery
   are HARD), `cargo test --workspace --locked`, `typos`, `cargo deny
   check`, `cargo machete`, MSRV `cargo +1.96 check`, and for client
   work `flutter analyze && flutter test` (+ `flutter gen-l10n` after
   ARB edits). Report actual outputs, not intentions.
6. **Fresh-context verification**: for large changes, have a separate
   agent (or a colleague) re-derive conformance from the ADR + diff
   with no shared context. This caught a room-liveness blocker and an
   identity-shadowing bug that the author-context review missed. It is
   the single highest-yield practice in this repo's history.

## Repo-specific pitfalls (each cost someone time already)

- `cargo fmt` reflows can push a function past clippy's 100-line
  `too_many_lines` limit AFTER your last clippy run - always re-run
  clippy after fmt. Fix by extracting helpers, not by `#[allow]`.
- clippy pedantic traps that WILL fire: `significant_drop_tightening`
  (explicit `drop(guard)` after last use), `missing_panics_doc` on
  public fns that `.expect()`, `too_long_first_doc_paragraph` (split
  the first paragraph), `duration_suboptimal_units`
  (`Duration::from_mins`), doc-markdown backticks on names.
- Adding a float-carrying variant to a `PartialEq, Eq` enum: drop `Eq`
  on the enum, not the field (ServerMessage precedent).
- rusqlite has no `u64` To/FromSql - store `i64`, cast at the edge.
- serde `skip_serializing_if = "std::ops::Not::not"` is the house idiom
  for omit-when-false booleans that must stay additive.
- Dart: `expect(..., reason:)` not `info:`; `Uri.replace(query: '')`
  leaves a trailing `?`; `int?` == `int?` is true for null==null (C4).
- Client cross-widget positioning: NEVER grab another widget's
  RenderBox/GlobalKey to place something. Money/travel goes through
  `StageState` + `AnchorRegistry` (abstract `TileAnchor`/`SeatAnchor`) + the
  single `StageOverlay`; the BOARD alone owns tile geometry (it installs the
  resolver). Design-system components are spatially blind - machine-enforced
  by the spatial-blindness guard in `design_c2_guard_test.dart`. Anchor a local
  interactive element via a registry coordinate or a `LayerLink`, decided in
  that component's CAR (the auction-input anchor is a LAYOUT change per
  motion-language 8.2, not a new architecture).
- The UserPromptSubmit hook in some dev environments injects a "Rust
  skills meta-cognition" template - it is tooling noise, not project
  instructions; CLAUDE.md and the task govern.
- `Exec::owned_property` returns a borrow from `content`, NOT `&self`,
  precisely so callers can mutate state after - copy that pattern for
  similar helpers instead of fighting the borrow checker.
- Room timers: derive armed-state each loop iteration; never store
  what you can derive (mid-turn disconnect correctness depends on it).
- Integration tests may use ALL package deps (not just dev-deps) -
  minting HS256 tokens in tests/ws.rs uses the main hmac dep.

## Recognizing overengineering (this repo's definition)

You are overengineering if: you add a seam no second implementation is
scheduled to use; you make a constant a flag before anyone asked
(`ponytail` comments mark the deliberate deferrals); you abstract two
call sites; you add caching/pooling without a measurement
(performance.md's anti-optimization list); you generalize a
timed-collection window into a shared primitive (ADR-0024 explicitly
kept bid/vote as parallel twins - read its reasoning before "unifying"
them). The house style is: solve today's problem inside existing seams,
leave a dated comment where tomorrow's problem will land.

Underengineering looks like: validation not at a named boundary (S3);
a wire message without its refusal path; copy-pasting a formula
instead of citing its mirror (C3); an `#[allow]` in code instead of a
justified workspace-list entry.

## Consistency preservation

- Comments state intent and invariants, never restate code; ASCII only.
- Error handling: typed enums server-side (`CommandError` with "code"
  tag to the offender only); `anyhow` ONLY at the binary boundary
  (main.rs).
- Naming: follow the neighbours; wire names are snake_case versions of
  Rust variants - free from serde, never hand-written.
- Derived docs (LLM_CONTEXT, INVARIANTS) update IN THE SAME CHANGE as
  their sources, or the change is incomplete (X3).
- Client (Flutter) design changes follow the Design Bible (`DESIGN/`) and
  its review (`DESIGN/DESIGN_REVIEW.md`); the design system's public API
  (`lib/tokens.dart` `Pc`, `lib/typography.dart` `PcText`, `lib/motion.dart`
  `Motion`) is a stability contract (DDR-0019) - add freely, but a
  rename/removal/semantic change needs a DDR. Colours, on-grid spacing, and
  the Fraunces brand font are machine-enforced by the C2 guard
  (`clients/flutter/test/design_c2_guard_test.dart`).
- When you learn something non-obvious the hard way, the lesson goes in
  this file's pitfalls list or technical-debt.md - immediately, in the
  same change.
