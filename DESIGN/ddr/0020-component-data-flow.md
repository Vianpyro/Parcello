# DDR-0020: component data flow - a Semantic Model + layered transient state

Status: RATIFIED (owner, 2026-07). Supersedes the rejected two-object
(`Data` + `Presentation`) proposal (see below).

## Context

A proposal: every L3/L4 domain component receives EXACTLY TWO immutable objects
- `Data` (business state, computed/formatted/localized, zero engine references)
and `Presentation` (all transient visual state: focus, hover, selection,
animation, reduced motion, accessibility, highlight...), a "UI director"
producing `Presentation`, components purely presentational and never deciding
animations.

The GOALS are right and already half-realized here: `SeatTile` (CAR-0001) is
presentational and engine-free; `director.compile` is a pure `(events, ctx) ->
Plan` with the accessibility knob as an EXPLICIT input (`CompileCtx.profile`,
ADR-0030); `stage.dart` is a transient-visual notifier deliberately SEPARATE
from `GameSession` so animation frames never repaint input fields. The question
is not "should components be presentational" (yes) but "what exactly do they
receive, and who owns each kind of transient state".

## Problem

Does bundling ALL transient visual state into one externally-produced
`Presentation` object per component hold up over a 10-year maintenance horizon
- and how do we keep components skinnable?

## Decision

**Keep the business-state half (renamed the Semantic Model); reject the
mandatory `Presentation` object; hand each kind of transient state to its
natural Flutter owner.**

### 1. One mandatory Semantic Model per component

Every L3/L4 component takes ONE immutable **Semantic Model** (the term is
deliberate: it describes the role better than "Data"). It is:

- **engine-free**: `GameSession`/`Player`/`Property`/`Auction`/`ClientView` -
  and any engine view type - NEVER appear in it;
- **pre-localized**: it carries finished, localized strings (no keys, no
  `AppLocalizations` inside the component, INVARIANTS C1);
- **STRICTLY SEMANTIC - no rendering information, none** (owner, 2026-07,
  because Parcello WILL support skins long-term). The model carries only
  semantic identifiers, localized text, and computed domain numbers. It MUST
  NOT contain: colours, fonts/text styles, spacing/sizes, durations/curves,
  icons, asset paths, or any other pixel/visual choice. Those belong to the
  component + tokens, so a future SKIN restyles by swapping tokens without
  touching a single model. (Example: `SeatTile` receives `seat: int`, and
  resolves `pawnColor(seat)` INTERNALLY from tokens - it never receives a
  colour.)

A `mapper` (app layer, engine- and l10n-aware) produces the model. This is
`SeatTile`'s existing param list, to be promoted to a named frozen type
(DDR-0019) as models multiply.

### 2. Transient visual state is LAYERED to its natural owner, not one object

- **Ambient environment** (reduced motion, text scale, high contrast,
  directionality): read from `MediaQuery`/`Theme` via context. Never pushed - an
  InheritedWidget distributes it optimally and without staleness. (Where a PURE
  function needs it - the director - it is threaded explicitly, as
  `CompileCtx.profile` already is. Purity, not ambience, is the reason there.)
- **Local interaction** (hover, focus, pressed, tap-to-select): framework-owned
  (`WidgetState`, `FocusNode`, `MouseRegion`). Discovered by the widget, not
  known by any director; externalizing it re-implements `FocusManager` and a
  hover round-trip per frame.
- **Orchestrated game motion** (chit travel, bid reveal, arrest): the existing
  `director -> Plan -> stage` pipeline. Components expose ANCHORS / slots
  (`SeatTile`'s `anchorKey`, `trailingBid`) for the stage to target; they do not
  receive per-frame presentation. Slots and anchors are NOT part of the semantic
  model - they are the orchestration seam.
- **Micro-transitions** (row highlight, chip select): a LOCAL implicit animation
  driven declaratively by a Semantic-Model change (`active: true`), timed by
  `Motion` tokens (`SeatTile`'s `Motion.stateFade`). Motion owns timing, so the
  component still "never invents a duration" without an external object.
- **Externally-commanded cues** (rare, imperative "flash now"): a small explicit
  cue, never a mandatory second object.

### 3. Intents are explicit and separate

`onAccept`, `onTap`, `onCancel` are callbacks the component emits - part of
neither the Semantic Model nor any presentation object. A presentational
component still emits events; the original proposal omitted this third leg.

## Invariant (skins)

**A component's Semantic Model contains ZERO rendering information.** Rendering
lives in the component + `Pc` tokens + `Motion` + `PcText` only. This is what
makes a skin (an alternate token set) possible without editing a single model
or component. Enforced by a source-scan in the C2-guard family: nothing under
`lib/design/` may import `session.dart` or the engine view types (`protocol.dart`),
and a Semantic-Model type carries no `Color`/`TextStyle`/`EdgeInsets`/`Duration`/
`IconData` field.

## Consequences

Positive:
- The engine boundary that gives the real wins is explicit: **previews /
  Showcase, golden tests, replays, spectators** render a component from a plain
  Semantic Model with no engine present (a replay is a stream of model
  snapshots; a spectator is `for_spectator` -> mapper -> the same component).
- **Skin/mod-ready**: models are semantic, tokens own pixels; mods feed content
  through the mapper (the board already renders any tile count).
- **Idiomatic Flutter, portable to touch/mobile**: hover is a no-op on touch,
  focus adapts, no platform knowledge leaks into a director.
- **No per-frame god-object**: no immutable `Presentation` re-allocated each
  hover/tick (GC on Steam Deck / future mobile), and the stage/session split is
  kept (animation frames do not repaint unrelated subtrees).
- **Text-input-safe**: `AuctionWidget` / `SettingsField` own their
  `TextEditingController` (user-owned, mid-edit) - a purely externally-driven
  presentation would reintroduce the reseed bug guarded by `bid_input_test`.

Negative / costs accepted:
- N frozen Semantic-Model types + N mapper functions to maintain (mitigated: the
  mapper is one file per surface; the model is the param list you already write).
- The "no engine type, no rendering info" boundary must be GUARDED, not just
  asked - hence the source-scan invariant above.

## Rejected alternative - the mandatory (Data, Presentation) two-object model

Rejected because it conflates "WHAT visual state to show" (declarative, fine to
externalize) with "HOW to transition and interact" (imperative, ticker- and
focus-bound, inherently local). It would externalize hover/focus (re-implementing
`FocusManager`; a hover round-trip per frame); turn the director into a 60fps
emitter of immutable objects OR make the component run the tween anyway
(contradicting "never decide animations"); duplicate `MediaQuery` (staleness);
allocate a `Presentation` per frame (GC); force an empty `Presentation` on
stateless components (`MoneyChit`); and break text input
(`AuctionWidget`/`SettingsField`). For ~6 domain components it is a large,
Flutter-fighting apparatus whose only unique property - external control of ALL
visual state - is a cost, not a benefit.

## Components that do not fit the two-object model (evidence)

- `AuctionWidget`, `SettingsField`: own a `TextEditingController` (mid-edit user
  state) - cannot be a pure function of pushed presentation.
- `TradeOfferCard`, `PropertyCard`: need intent callbacks - no home in "Data" or
  "Presentation".
- `MoneyChit`: no transient state at all - a mandatory `Presentation` is empty
  ceremony.

## Enforcement / integration

CAR sections 2 (Boundaries), 6 (Motion), 7 (Accessibility) are the per-component
application of this DDR; they use the term Semantic Model and cite it. CAR-0001
(`SeatTile`) is conformant as-is (it takes a semantic param list + the
`anchorKey`/`trailingBid` slots, no rendering info). The source-scan lands in
`test/design_c2_guard_test.dart`'s family.

The orchestration seam named in section 2 (components expose abstract anchors;
`stage.dart`/`overlay.dart` own cross-widget placement) is now machine-enforced
by the sibling **spatial-blindness guard** (`lib/design/` never resolves widget
geometry). No separate DDR governs scene composition: the invariant lives in
`stage.dart`/`overlay.dart` + this seam + that guard, and each anchored
component states its placement path in its own CAR (section 2). A dedicated DDR
is warranted only if a SECOND interactive-anchored component needs a shared
decision the CARs cannot each carry.
