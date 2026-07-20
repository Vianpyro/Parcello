# Implementation roadmap

How to turn the Design Bible into shipped Flutter, in the right order:
foundations before components, components before screens, screens before
polish. This is a BUILD-ORDER document (distinct from
`docs/roadmap-and-product.md`, which sequences product/features, and
`DESIGN/COMMERCIAL_UX_AUDIT.md`, which prioritizes gaps by player
impact). It is also the **maintained source of built-vs-not-built truth**
for the client - it supersedes `docs/motion-language.md` section 13's
honesty list, which drifted (fonts et al. were wrongly carried as unbuilt).

Principle (the owner's, and correct): **build the SYSTEM, not screens.**
Each PR should leave behind a reusable brick, not a one-off. A good build
order saves weeks by never rewriting a component that a later screen
reuses.

Keep this file current: when a phase ships, update its status here in the
SAME change (the X3 discipline). If the two ever disagree, the code wins
and this file has a bug.

---

## Ground truth (audited 2026-07)

What already exists is a lot - the motion architecture and the palette
landed early. Do NOT rebuild these.

**Built and solid:**

- **Tokens (partial)**: `lib/tokens.dart` - full palette, group/pawn
  colours, radius (2 px), the hairline shadow. (Missing: a named spacing
  scale and typography roles - see Phases 1-2.)
- **Fonts**: Fraunces / Inter / Source Serif 4 bundled under
  `assets/fonts/` (licences + SHA256SUMS), `fontFamily: 'Inter'` in the
  theme, the other two applied at use sites. DONE.
- **Motion spec**: `lib/motion.dart` - tiers, lanes, profiles, the tiered
  budget, every duration and curve (including `reorient`, defined but
  unused).
- **Animation engine**: `lib/stage.dart` (transient state, anchors,
  chits, the three attention devices), `lib/director.dart` (pure
  `compile()`, budget rule, coalescing, ~28 event beats), `lib/overlay.dart`
  (travelling chits, the P1 arrest), `reveal.dart`, `flashes.dart`. The
  profile knob and Escape-to-skip work. Guarded by `director_test.dart`,
  `stage_render_test.dart`.
- **Board**: `lib/board.dart` - palette applied, recede/lift/frame,
  threat flash, band sweep, refusal shake, market-adjusted prices. The
  auction tile ALREADY lifts and the board recedes.
- **Beats compiled** (director cases): movement card, move, jail hop,
  card draw, every money path (salary, **rent-to-the-earner**, tax, card
  cash, build, boost, mortgage, redeem, liquidation), auction open,
  auction resolve with bids face-up, **the discoverer rebate chit**, band
  sweep, expropriation, the sprung boost trap, spotlight, market event,
  bankruptcy, all five win conditions.
- **l10n**: gen-l10n, EN + FR, every string an ARB key. DONE.
- **Screens (functional, ad-hoc styling)**: connect, menu, lobby, game,
  finished, rules, spectate; the CLI harness.

**Genuinely NOT built (the real remaining work):**

- No **named spacing scale** or **typography role** helpers - spacing is
  inline `EdgeInsets`/`SizedBox` magic numbers, type roles are inline
  `TextStyle(...)` per widget. (Phases 1-2.)
- No **coherent component library** - buttons (`wideButton`), tiles,
  chips, cards, dialogs are ad-hoc widgets scattered across `lib/ui/`,
  not a named set with variants/states/tests. (Phases 5-6.)
- The **auction input is not anchored to the tile**; the window clock is
  a corner number, not a hairline draining on the tile's own edge. (The
  tile-lift half IS built.) (Phase 6 - the hardest widget, deliberately
  late.)
- **Trade animations** (no `trade_*` beat in the director - log only),
  **bribe reveal** as a banner not votes-flipping-face-up (a beat exists,
  the treatment differs from spec), **AFK auto-play marker**,
  **time-bank P2 alarm**, **bot-thinking pulse**, **hand-refill beat**,
  **reconnect re-orientation** (`Motion.reorient` unused). (Phases 4/8.)
- **Icons**: still Material default; Tabler + in-house glyphs decided
  (DDR-006, owner-confirmed 2026-07) but not migrated. (Phase 5.)
- **Accessibility**: no screen-reader Semantics; no high-contrast
  profile; the CVD palette audit is unrun. (Phase 9.)
- **Board is flat**, not isometric - and DELIBERATELY stays flat for now
  (DDR-017, owner-confirmed 2026-07). (Out of this roadmap's core path.)
- **Audio**: placeholder clips; deferred until the events exist (owner
  decision 2026-07) - see Phase 8's note.

---

## Phase 0 - Structure decision (do this before any code)

**Goal**: decide WHERE the design system lives, so every later phase has a
home. No behaviour change.

The owner's instinct - build the system as a first-class layer before
screens - is right and drives this whole roadmap. The open question is
whether that layer is a **separate Dart package**
(`packages/parcello_design/`) or an **in-tree layered folder**
(`lib/design/`). This is a structural decision and gets a DDR (DDR-016).

**Recommendation: in-tree `lib/design/`, NOT a separate package - yet.**
Reasoning:

- There is exactly ONE consumer (the app). A package's value is
  multi-consumer reuse, an enforced API boundary, and independent
  versioning - none of which applies with one app. Extracting a package
  no second consumer needs is the textbook over-engineering the technical
  bible warns against ("a seam no second implementation is scheduled to
  use").
- A package split is a real restructure with real cost and risk: a second
  pubspec + path/workspace wiring, `flutter.yml` CI changes, and rewriting
  imports across ~45 files - churning code that is green today
  (`director_test`, `layout_test`, `bid_input_test`, `stage_render_test`,
  `spectate_and_hints_test` all pass against the current layout) for zero
  user-facing gain.
- The foundations mostly EXIST already (`tokens.dart`, `motion.dart`,
  `stage`, `director`); the work is to COMPLETE and ORGANIZE them, which a
  folder does as well as a package.
- The boundary the owner wants is already a documented invariant (C2: "a
  hex or duration literal at a use site is a bug"); a folder + a lint
  enforces it without package ceremony.
- **Clean extraction path preserved**: IF a second consumer ever appears -
  a standalone **replay viewer** is the plausible one (LEGACY + product
  roadmap name it; the replay format already exists) - THEN extract
  `lib/design/` into `packages/parcello_design/`. That is the moment the
  seam earns its keep, and it is a mechanical move once the folder
  boundary is already clean.

**Target in-tree layout:**

```
lib/design/
  tokens.dart        <- palette + spacing scale + radius + elevation (grows from today's tokens.dart)
  typography.dart    <- named text roles (display/section/body/amount/tile-name)
  motion.dart        <- moves here unchanged (already the spec)
  theme.dart         <- ThemeData assembly + a PcTheme extension for non-Material tokens
  components/        <- PcButton, PcPanel, PcDialog, PcCard, PcChip, PcListRow, PcBadge, ...
  composite/         <- PropertyCard, PlayerCard, AuctionWidget, TradePanel, MoneyChit, Marker
  animation/         <- stage.dart, director.dart, overlay.dart, reveal.dart, flashes.dart move here
lib/ui/              <- screens only, consuming the above
```

**If the owner prefers the package** (e.g. a replay viewer is genuinely
imminent): the phase order below is IDENTICAL; only Phase 0 changes -
add a `packages/parcello_design` bootstrap step first, wire the path
dependency and CI, and every later phase targets the package instead of
`lib/design/`. Nothing downstream reorders.

**Decided (owner, 2026-07): in-tree `lib/design/`.** DDR-0016 accepted as
a DEFERRAL of package extraction, not a refusal - extraction is expected
when any explicit criterion fires (a second consumer / replay viewer /
companion app, design-system stabilization, or the in-tree boundary not
holding; see the DDR). Consequence for this roadmap: keep the
`lib/design/` folder boundary deliberately CLEAN from Phase 1 on (C2 + a
lint), so the eventual extraction stays mechanical.

- **Validation**: the decision is recorded (DDR-0016, accepted) and the
  target structure agreed. Docs match reality (this file). No code moved
  yet.
- **Risk**: none (decision + docs). **Rollback**: trivial.

---

## The phases

Each phase: **goal / already built / to build / depends on / risk /
validation / rollback**. Statuses: NOT STARTED unless noted.

### Phase 1 - Design tokens (complete them)

- **Goal**: one source for every colour, space, radius, elevation - no
  magic numbers at use sites.
- **Already**: colours, group/pawn, radius, hairline shadow.
- **To build**: a named **spacing scale** (`Pc.space4/8/12/16/24`, or a
  helper), elevation/border helpers; then migrate inline
  `EdgeInsets`/`SizedBox` magic numbers to it, widget by widget.
- **Depends on**: Phase 0.
- **Risk**: LOW (additive; migrate incrementally).
- **Validation**: no raw spacing literals in migrated widgets; `flutter
  analyze` clean; `layout_test` green at all three sizes.
- **Rollback**: mechanical revert (additive tokens + use-site edits).

### Phase 2 - Typography roles

- **Goal**: named text styles replace scattered inline `TextStyle(...)`;
  tabular figures on live numbers enforced centrally.
- **Already**: fonts bundled; Inter as the theme family.
- **To build**: `typography.dart` with the roles from TYPOGRAPHY.md
  (display=Fraunces, section/body/caption/amount-tabular=Inter,
  tile-name=Source Serif 4); migrate use sites.
- **Depends on**: Phase 1.
- **Risk**: LOW-MED (a wrongly mapped style is a visible regression - use
  screenshot diffs).
- **Validation**: no inline `fontSize`/`fontWeight` in migrated widgets;
  screenshot parity at 1280x800 and 1024x600.
- **Rollback**: revert `typography.dart` + migrations.

### Phase 3 - Theme consolidation

- **Goal**: components inherit shape/colour/type from ONE `ThemeData` +
  a `PcTheme` extension (chit colours, hairlines, the non-Material
  tokens), instead of per-widget overrides.
- **Already**: `ThemeData` with seed + surface + `fontFamily` + a radius
  override (main.dart).
- **To build**: button/card/dialog/input themes pushed into the theme;
  the `PcTheme` extension.
- **Depends on**: Phases 1-2.
- **Risk**: MED (theme changes ripple app-wide).
- **Validation**: every screen renders identically pre/post (screenshot
  sweep); `layout_test` green.
- **Rollback**: revert the theme; widgets keep their local styles until
  Phase 5 proves the theme (do NOT delete local styles until then).

### Phase 4 - Animation primitives (mostly done - fill the gaps)

- **Goal**: the reusable motion vocabulary complete; the few missing
  primitives added to the existing engine.
- **Already**: the whole director/stage/overlay engine, 28 beats, the
  budget, the profile knob, tests. This phase is SMALL.
- **To build**: a **persistent-marker** primitive (for AFK / time-bank -
  a marker is not a beat, it is durable stage state), a **pulse**
  primitive (bot-thinking), and wiring `Motion.reorient` into a
  **re-orient** beat. No new attention device (frame/lift/recede
  suffice - a fourth would need a DDR).
- **Depends on**: Phases 1-3 (tokens for styling).
- **Risk**: LOW-MED (the director is well-tested; every new beat gets a
  budget test).
- **Validation**: `director_test` extended per primitive; the budget
  invariant still holds; profiles (Instant especially) still apply.
- **Rollback**: additive - remove the new primitives.

### Phase 5 - Core widgets

- **Goal**: the named, reusable component set of DESIGN_SYSTEM.md,
  normalizing today's ad-hoc widgets.
- **Already**: `wideButton`, `menu_tile`, `bid_chip`, `coach_mark`,
  `feedback_card` - ad-hoc, to be folded in.
- **To build**: `PcButton` (primary/secondary/destructive/quiet + chip),
  `PcPanel`, `PcDialog`, `PcCard`, `PcChip`, `PcListRow`, `PcBadge`; each
  with states (rest/hover/pressed/disabled/focus) and a widget test.
  Migrate the Tabler icon set here (DDR-006).
- **Depends on**: Phases 1-3.
- **Risk**: MED (touches every screen as they migrate - do it screen by
  screen, not big-bang).
- **Validation**: a test per component; `layout_test` green throughout;
  DESIGN_REVIEW checklist per component.
- **Rollback**: components are additive; migrate screens incrementally so
  any single revert is small.

### Phase 6 - Composite widgets

- **Goal**: the game-specific compositions, including the hardest one.
- **To build**: `PropertyCard` (parchment, edge band, rent ladder),
  `PlayerCard`/`SeatPanel` (the chit TARGET - fixed position is
  load-bearing), `MoneyChit` (formalize from overlay), `Notification`/
  `Marker`, `TradePanel`, and **`AuctionWidget`** - the anchored input +
  the tile-edge draining clock (motion-language 8.2). This is THE
  high-risk widget and it is deliberately here, not first.
- **Depends on**: Phases 4-5.
- **Risk**: HIGH for `AuctionWidget` (the 1024x600 floor, `bid_input_test`,
  the half-typed-bid guards, and the E5 masking invariant - a spectator/
  rival must NEVER see a pending bid). Others: MED.
- **Validation**: `bid_input_test` + `layout_test` + a NEW auction-anchor
  test; screenshots at both sizes; E5 masking re-verified against the
  server view; the coach-mark/onboarding still reads.
- **Rollback**: keep the current centre-panel bid input as the fallback
  until the anchored one is proven (a one-widget swap, gated behind a
  local flag if needed); revert is contained to `AuctionWidget`.

### Phase 7 - Screens (consume the components)

- **Goal**: each screen rebuilt ON the component set, not ad-hoc widgets.
- **Already**: all screens exist and function - this restyles/re-wires.
- **To build**: migrate connect, menu, lobby, game, finished, rules,
  spectate (and the future ranked-queue screen) to the components; apply
  SCREEN_ARCHITECTURE rules (one primary action, fixed positions, no
  reflow on state change).
- **Depends on**: Phases 5-6.
- **Risk**: MED (per-screen; incremental).
- **Validation**: `layout_test` at three sizes WITH localized strings
  (the longer of EN/FR); DESIGN_REVIEW per screen; screenshot diffs.
- **Rollback**: per-screen, incremental.

> **Playtest gate**: after Phases 6-7 the game is presentable enough for
> the first real multi-human playtest (the roadmap-and-product Critical
> item). FREEZE tuning-sensitive values (window durations, the budget,
> matchmaking constants) until playtests speak - this roadmap is about
> APPLYING the design, not re-tuning it.

### Phase 8 - Gameplay polish (the felt-quality gaps)

- **Goal**: close the remaining motion/UX holes on the now-solid infra.
- **To build**: AFK auto-play marker, time-bank P2 alarm, bot-thinking
  pulse, hand-refill beat, reconnect re-orientation, trade animations
  (`TradeProposed`/`Accepted` beats), bribe reveal as votes-flipping-
  face-up (replace the banner). **Audio pass slots here or just after**
  (owner deferred it until the events exist - now they do): wire the four
  category earcons (AUDIO_DIRECTION), replace the `dice-roll` stand-in.
- **Depends on**: Phases 4-7.
- **Risk**: LOW-MED (additive, each tested; respect per-observer tiers -
  being attacked is louder for the victim).
- **Validation**: `director_test` per new beat; the "played for whom"
  rules (motion-language 8.3); audio never the sole channel.
- **Rollback**: additive, per-feature.

### Phase 9 - Accessibility

- **Goal**: meet the ACCESSIBILITY.md guarantees that are still open.
- **To build**: Flutter Semantics on the board + log (the localized log
  is the tractable seed for a screen reader), a high-contrast profile,
  the CVD palette audit + an alternate group/pawn set if needed, a
  relaxed-timer preset (DDR-015), a text-scaling pass.
- **Depends on**: Phases 5-8 (Semantics wrap finished widgets).
- **Risk**: LOW (additive).
- **Validation**: the ACCESSIBILITY review question (Instant motion / CVD
  / controller / 1024x600 / screen reader); a manual SR pass.
- **Rollback**: additive.

### Phase 10 - Optimization (only if measured)

- **Goal**: nothing, until a profiler says otherwise (performance.md:
  fixed camera, <=6 seats, no known bottleneck).
- **Candidate (if profiled)**: serialize the spectator view once per
  update instead of per watcher (contained to `send_spectators` - and it
  is a SERVER change, out of this client roadmap's core).
- **Depends on**: a measurement, not a phase.
- **Risk**: only ship a measured win. **Rollback**: N/A.

---

## Dependency graph (critical path)

```
Phase 0 (decide)
  |
  +-- 1 tokens --+
  +-- 2 type   --+--> 3 theme --> 5 core widgets --> 6 composite --> 7 screens --> [PLAYTEST]
  |              |                     ^                                              |
  +-- 4 anim primitives (small) ------+                                              |
                                                              8 polish + audio  <-----+
                                                              9 accessibility   <-----+
                                                              10 optimization (if measured)
```

Critical path: **0 -> 1/2 -> 3 -> 5 -> 6 -> 7**. Phase 4 is off the
critical path (small, parallelizable). Phases 8-10 depend on 7. The
AuctionWidget (Phase 6) is the single highest-risk node and the reason
the whole order exists: it is reached only after tokens, type, theme,
core components and the animation primitives it needs are all proven -
never as the first Flutter chantier.

## Cross-cutting rules (every PR, every phase)

- Leaves behind a reusable brick, not a one-off (the whole point).
- Green CI: `flutter analyze` + `flutter test` + web build; `flutter
  gen-l10n` after any ARB change; new strings in BOTH ARB files.
- `layout_test` stays green at 1280x800 / 1280x720 / 1024x600 with the
  longer localized strings; a pumped overflow is a failure.
- Screenshot diff at 1280x800 AND 1024x600 for any visual change.
- No hex/duration literal at a use site (C2); it goes in tokens/motion.
- Runs the DESIGN_REVIEW checklist; a change that alters a bible rule
  ships its DDR + the bible update in the same PR.
- Never shows what the server masks; never implies unsent state; never
  moves the camera (the three walls, SELF_CRITIQUE).
- Migrate incrementally (screen by screen, widget by widget) so any
  single revert is small - never a big-bang restructure.

## Status log (keep current)

| Phase | Status | Notes |
|---|---|---|
| 0 Structure decision | DONE | in-tree `lib/design/` (DDR-0016 accepted; package extraction deferred w/ criteria) |
| 1 Tokens | IN PROGRESS | spacing scale (`Pc.s2..s24` + `cardInset`) landed in `lib/tokens.dart`; connect + menu + menu_tile migrated as proof (value-preserving, pixel-identical, 50 tests green); off-grid one-offs (e.g. a lone `10`) left literal by policy. Remaining: migrate the other widgets incrementally + an elevation helper if a use case appears + the C2 lint once coverage is high |
| 2 Typography | NOT STARTED | roles missing; fonts done |
| 3 Theme | NOT STARTED | base ThemeData exists |
| 4 Anim primitives | PARTIAL | engine done; marker/pulse/re-orient missing |
| 5 Core widgets | NOT STARTED | ad-hoc widgets to normalize; Tabler icons |
| 6 Composite (incl. AuctionWidget) | NOT STARTED | highest risk; auction anchor is #1 gap |
| 7 Screens | FUNCTIONAL, UNSTYLED-TO-SYSTEM | exist; consume ad-hoc widgets today |
| 8 Polish + audio | NOT STARTED | AFK marker, trade/bribe, alarms, earcons |
| 9 Accessibility | NOT STARTED | Semantics is the big one |
| 10 Optimization | DEFERRED | only on a measurement |
