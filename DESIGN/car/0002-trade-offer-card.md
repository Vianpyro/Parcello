# CAR-0002: TradeOfferCard

Status: RATIFIED (owner, 2026-07)
Level: L3    Inventory: pending    Pulled by: side panel trade list (replaces the
sentence-based `tradeOffer` string)

> This CAR formalizes a decision already reached in design discussion (a
> structured-vs-sentence review, then a self-challenge of the initial
> two-column recommendation). No new design decision is made here; this is
> the record the discussion should have produced before code, per the CAR
> gate (CAR-0001 precedent).

**DDR-0020 conformance (by design, not retrofit):** TradeOfferCard takes a
strictly semantic param list - seat indices, pre-formatted strings, and
nullable action callbacks - and resolves NO game state, NO localization, and
NO `Color` values from its caller. Like `SeatTile` resolving `pawnColor(seat)`
internally from a bare `int`, this component resolves `groupColors[group]`
internally from a bare group key. It holds zero rendering information passed
in as data.

## 1. Responsibility

Render ONE pending trade offer as a structured card: who proposes to whom,
what is given, what is requested, and the actions available to the current
viewer (accept/decline as recipient, cancel as proposer, or neither as a
non-party - though ADR-0007 means a non-party never actually receives an
offer to render).

It does NOT:
- fetch or derive game state (`GameSession`/`ClientView` never touch it);
- decide WHICH actions are available (accept/decline/cancel is a permission
  question the parent already computes today - `o.to == s.seat` /
  `o.from == s.seat` - the card only renders whichever callbacks it is
  given, `null` = hidden);
- format currency, compose the mortgaged-tile suffix, or localize any string
  (the parent does, exactly as `side_panel.dart`/`trade_dialog.dart` already
  do today for `'\$$cash'` and `tileName + ' (M)'`);
- own the confirm/undo of an action (accept/decline/cancel fire immediately,
  matching today's behaviour - no new confirmation step is introduced).

## 2. Boundaries

- **Layer**: `lib/ui/game/trade_offer_card.dart` - a domain composite (like
  `PropertyPanel`), NOT `lib/design/components/` (reserved for base `Pc*`
  primitives) and NOT a new shared cross-screen component (only one
  consumer exists today, per the pull-based/anti-speculation rule).
- **Imports**: `tokens.dart`, `typography.dart`, and the DS components
  (`PcCard`, `PcButton`) only. **Never** `session.dart` or
  `l10n/app_localizations.dart` - stricter than `PropertyPanel` (which does
  resolve `GameSession`/`AppLocalizations` internally) and matching
  `SeatTile`'s boundary instead, because this task's brief explicitly asks
  for presentation-only DDR-0020 conformance.
- **Inputs**: seat indices (`fromSeat`, `toSeat` - pawn colour resolved
  internally), already-localized names, already-formatted cash strings,
  already-composed tile display names (mortgage suffix included by the
  caller, exactly as `TradeDialog` already composes `' (M)'` today) paired
  with a raw group key (colour resolved internally), and three nullable
  `VoidCallback`s.
- **Ownership split**: the parent (`_trades()` in `side_panel.dart`) keeps
  computing permissions, formatting cash, composing tile names, and
  resolving player names - unchanged from today. It only stops building the
  sentence and the ad hoc `Column`/`Row`, and constructs `TradeOfferCard`
  instead.

## 3. Public API

```dart
const TradeOfferCard({
  required int fromSeat,
  required String fromName,
  required int toSeat,
  required String toName,
  required String giveCash,     // '' = nothing given in cash
  required List<({String name, String? group})> giveTiles,
  required String receiveCash,  // '' = nothing requested in cash
  required List<({String name, String? group})> receiveTiles,
  required String nothingLabel, // localized fallback when a side is empty (t.tradeNothing)
  VoidCallback? onAccept,
  VoidCallback? onDecline,
  VoidCallback? onCancel,
})
```

Tile entries use an anonymous Dart record (already an established pattern in
this codebase - `nav_rail.dart`'s `_objectives` rows), not a new named model
class - avoids introducing a type for a two-field, call-site-only shape.

`nothingLabel` is required (not a hardcoded fallback string) because
INVARIANTS C1 forbids any literal user-facing text inside a widget, even a
one-word fallback - it must arrive from the caller's `AppLocalizations`,
exactly like every other string this component receives.

## 4. Invariants

- **Presentational only**: identical params render identically; no I/O, no
  ambient state read.
- **Absent-means-hidden** for actions: `onAccept`/`onDecline`/`onCancel` are
  independently nullable; the parent decides which are non-null exactly as
  `_trades()` does today (recipient gets accept+decline, proposer gets
  cancel, never both - a party cannot be both `from` and `to` of the same
  offer).
- **Identity row is always present**: proposer pawn + name (left), recipient
  pawn + name (right) - left-to-right IS the direction cue, no icon needed
  (no "swap"/"arrow" icon exists anywhere in the project's current icon
  vocabulary, and none is added here).
- **Body is a single vertical flow** (DONNE, then REÇOIT), never two
  side-by-side columns - the deciding factor from the self-challenge: the
  engine places no cap on tiles per offer
  (`MAX_OPEN_TRADES_PER_PLAYER` caps offer COUNT, not tiles within one), so
  two columns can be arbitrarily unequal in height inside a single flat,
  undecorated `PcCard` (ART_DIRECTION forbids a dividing rule/shadow to
  visually justify the asymmetry) - a vertical flow degrades to any content
  size without looking broken.
- **One-line collapse rule** (the only conditional layout branch): a side
  with exactly one tile and non-empty cash renders on ONE line (amount +
  tile, inline); any other combination (0 or 2+ tiles, or cash-only, or
  tile-only) stacks each item on its own line. This recovers the two-column
  layout's scan speed for the common case (one tile + cash each side)
  without its worst-case height risk. This rule is fixed here, not left to
  call-site discretion.
- **Group colour band reuses PropertyPanel's VISUAL pattern, not its code**:
  a `Pc.s4`-wide `Container` coloured `groupColors[group] ?? Pc.textFaint`
  beside the tile name - the same idea as PropertyPanel's header band, built
  fresh as a small private widget in this file (per CAR discussion:
  `PropertyPanel` exposes no reusable sub-widget, and forcing reuse would
  mean importing a full detail panel for one fragment of it - the wrong
  shape). Promote to a shared file only if a THIRD real consumer
  demonstrates the same need later (pull-based rule, DDR-0019 precedent).

## 5. Extensibility

- Anticipated additive (defaulted) growth, each on real demand only: a
  `dense` density if a second, tighter host ever needs it (mirroring
  `PcButton`/`PcTextField`'s precedent); a status badge (e.g. "waiting on
  you") if a future screen wants triage at a glance - NOT built now, no
  screen demands it.
- **Would need a new CAR**: collapsing the card behind a
  summary/expand toggle (the rejected Structure C) - it would introduce
  local widget state and change the responsibility from "always show
  everything" to "progressive disclosure", a decision, not a param.

## 6. Motion

None owned. No director beat, no bespoke animation - the card's content
changes only when the parent rebuilds it from a new `pendingTrades` list
(an ordinary Flutter rebuild, like today's `_trades()`). Reduced-motion has
nothing to opt out of here.

## 7. Accessibility

- **Default text/button semantics only** - like `SeatTile` (CAR-0001 S7), a
  dedicated `Semantics` label is a future accessibility item
  (ACCESSIBILITY.md), not blocking here; `PcButton` already carries its own
  button semantics.
- **No colour-only signal**: identity is pawn colour AND name text; group
  identity is band colour AND tile name text - never colour alone.
- **Keyboard/controller**: the three action buttons are ordinary `PcButton`s
  in a `Row`, focus-traversable like every other button on the panel today -
  no new traversal group needed (the side panel is already a
  `SingleChildScrollView`, not a `FocusTraversalGroup` boundary of its own).

## 8. Localization

Receives every human-visible string already localized and formatted
(`fromName`, `toName`, `giveCash`, `receiveCash`, tile `name`s,
`nothingLabel`, `givesLabel`, `receivesLabel`); composes none, hardcodes none
(INVARIANTS C1). Most strings reuse existing keys unchanged
(`t.tradeNothing`, `t.actionAccept`, `t.tradeRefuse`, `t.cancel`). **Two new
ARB keys are required** for the section headers: the existing
`tradeYouGive`/`tradeYouWant` ("You give"/"You want") are perspective-locked
to the offer's COMPOSER and read false when this card is shown to the
recipient of someone else's offer - a gap only found while wiring this
section, not before. Added as neutral, offer-relative labels:
`tradeGivesLabel` ("Gives"/"Donne") and `tradeReceivesLabel`
("Receives"/"Reçoit"), always meaning "what the proposer gives/receives",
regardless of which party is looking at the card.

## 9. Dependencies & alternatives

- DS deps: `PcCard` (shell), `PcButton` (`quiet`, `wide: false` - unchanged
  from today's styling), `PcText` (`rowTitle`/`label`/`whisper`/`amount`),
  `Pc` tokens (`s2`/`s4`/`s6`/`s8`, `groupColors`, `pawnColor`,
  `Pc.textFaint`, `Pc.textMuted`).
- **Alternative considered and rejected: two-column DONNE/REÇOIT layout**
  (the initial recommendation). Rejected after self-challenge: its
  "consistency" precedent (`TradeDialog`) was verified to be raw
  `TextField`/`DropdownButton`/`CheckboxListTile`, NOT design-system
  code - not an actual consistency argument. Its real risk was not
  localization width (property names are mod content, locale-invariant,
  ~16-18 chars max, verified) but unbounded, uncapped tile count per offer
  producing ragged, unequal column heights inside a flat undecorated card.
- **Alternative considered and rejected: reusing `PropertyPanel` directly**.
  Wrong shape (a full detail panel resolving `GameSession`/`ClientView`
  itself) for a compact, pre-resolved list row; and it exposes no
  extractable sub-widget to reuse partially.
- **Alternative considered and rejected: summary + expand-on-tap (Structure
  C)**. Hides the specific properties behind a tap, contradicting the
  stated goal of understanding what is offered/requested at a glance; would
  introduce local widget state this component has no compelling reason to
  carry.

## 10. Testing

`test/trade_offer_card_test.dart`: constructs the card directly (plain
records/strings, no `GameSession`) verifying identity row renders both
names; the one-line collapse rule fires for the (cash + 1 tile) case and
does not fire otherwise; `onAccept`/`onDecline`/`onCancel` being null hides
their button; tapping a non-null callback invokes it.

Load-bearing: `test/layout_test.dart`'s fixture was extended (not just
uniform six cash-only offers) to include one offer stacking three
give-tiles - `TradeOfferCard`'s tallest shape, the "unequal column height"
risk this whole design avoided by going vertical instead of two-column
(S4). **Verified, not just accepted**: the panel does not overflow at any
of the three committed sizes, including the 1024x600 floor, with that
worst-case shape present alongside five other simultaneous offers.
