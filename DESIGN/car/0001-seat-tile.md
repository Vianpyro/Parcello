# CAR-0001: SeatTile

Status: RATIFIED (owner, 2026-07) - RETROACTIVE
Level: L3    Inventory: #13    Pulled by: Game HUD / side panel (DESIGN_FEEDBACK #4)

> Retroactive record: SeatTile shipped (migration #4) before the CAR gate
> existed. This CAR documents the as-built architecture and ratifies it, and is
> the worked example the template is validated against. No behaviour change.
>
> **DDR-0020 conformance (confirmed):** SeatTile already takes a strictly
> semantic param list (ids + localized strings + numbers) + the
> `anchorKey`/`trailingBid` slots + no intents (it is non-interactive). It holds
> **zero rendering information** - it receives `seat: int` and resolves
> `pawnColor(seat)` from tokens INTERNALLY, never a colour. When the param list
> is promoted to a named `SeatTileModel` type, that type inherits this invariant.

## 1. Responsibility

Render ONE player's seat row: identity (pawn colour + name), live figures
(cash, victory points, net worth), and per-seat state (whose turn, bankruptcy,
VP rank). It is also the fixed **target** a money chit flies to and the surface
a sealed bid flips face-up on.

It does NOT:
- fetch or derive game state (the parent reads `GameSession`/`ClientView`);
- compute cross-seat values - VP **rank** and the **round** metronome need all
  seats, so the parent computes them once and passes the result in;
- format or localize (no `$`/plural/number formatting, no `AppLocalizations`);
- own the chit's travel (that is the overlay/director) - it only exposes the
  anchor;
- decide WHICH status tags apply (domain logic; the parent joins them).

## 2. Boundaries

- **Layer**: `lib/design/components/` (in-tree DS, DDR-0016). Imports only
  `motion.dart`, `tokens.dart`, `typography.dart` - **never** `session.dart` or
  `l10n/`. This is what keeps it unit-testable with plain values and correctly
  layered (INVARIANTS C1 keeps l10n in the app layer).
- **Inputs**: already-resolved, already-localized **strings** (`cash`,
  `vpLabel`, `netWorthLabel`, joined `tags`) + plain flags/ints (`seat`,
  `rank`, `active`, `bankrupt`) + two stage handles (`anchorKey`, `trailingBid`).
- **Ownership split**: parent (`_players` in `side_panel.dart`) computes ranks +
  round, resolves the name, joins the tags, formats the money labels, and builds
  the `BidChip`; SeatTile only draws and places the anchor.

## 3. Public API

`const SeatTile({ required seat:int, required name:String, required tags:String,
required active:bool, required bankrupt:bool, cash:String?, vpLabel:String?,
netWorthLabel:String?, rank:int?, anchorKey:Key?, trailingBid:Widget? })`

Frozen (DDR-0019). Required: `seat`, `name`, `tags`, `active`, `bankrupt`. All
figure/state extras are nullable and absent-means-hidden (a lobby seat passes
none of the money fields; the whole figures column then disappears).

## 4. Invariants

- **Presentational only**: given identical params it renders identically; it
  triggers no I/O and reads no ambient game state. (Checkable: it builds under
  `_host` with no session in tests.)
- **`cash == null` <=> lobby seat** => the trailing figures column (cash / net
  worth / VP) is entirely hidden. `netWorthLabel`/`vpLabel` are only shown when
  non-null, and are only ever non-null alongside cash.
- **The `anchorKey` sits on the pawn circle** - money addressed to this seat
  must visibly arrive there (ADR-0028 chit targeting depends on it).
- **Bankruptcy uses TWO channels** (see A11y): dim + strikethrough, never colour
  alone.
- Value-preserving: it reproduced the pre-component seat row exactly (verified
  by `layout_test` at all three sizes).

## 5. Extensibility

- Anticipated additive (defaulted) growth, each on real demand: a `you`/`bot`
  emphasis if tags become richer; a `compact` density if a second, tighter host
  ever needs it.
- **Would need a NEW CAR/DDR** (not additive): turning the text `tags` into
  `PcBadge` pills (Visual Debt VD-1) - it changes the responsibility split (the
  component would own badge layout) and the look; a decision, not a param.

## 6. Motion

- Uses `Motion.stateFade` (200 ms) for the in-place highlight/dim of the row on
  turn/bankruptcy change - the ONLY animation it owns. No director beat; no raw
  `Duration` (added `Motion.stateFade` for it, migration #4).
- It is an animation **TARGET** (the `anchorKey`), not an animator - the chit
  travel and the bid reveal are the overlay/director's; SeatTile just holds the
  anchor and slots the `trailingBid` widget.
- Reduced motion: `AnimatedContainer` degrades to an instant state change, which
  is correct (the information - who is active - survives, the transition drops),
  consistent with the stage's reduced-motion contract.

## 7. Accessibility

- **Non-interactive** (a status display): no focus/traversal of its own, so it
  never competes with the action buttons for controller focus.
- **No colour-only signal**: the acting seat = highlight + a leading
  `play_arrow` marker + bold, not just gold; bankruptcy = dim + strikethrough,
  not just opacity; VP leader = a crown icon, not only position.
- **Text scaling / narrow width**: the name is `Expanded` + `ellipsis`, the
  figures are a fixed-width trailing column, so the row grows with zoom and
  clips the name (not the numbers) when narrow - covered by the Showcase
  narrow-160px case and by `layout_test`.
- Semantics: inherits default text semantics; a dedicated seat Semantics label
  is a future accessibility item (ACCESSIBILITY.md), not blocking.

## 8. Localization

Receives every human-visible string ALREADY localized (`name`, `tags`,
`vpLabel`, `netWorthLabel`); it composes none of them and hard-codes no text
(INVARIANTS C1). The `$rank` numeral in the pawn circle is a rank index, not
localized copy (a bare integer badge, like a jersey number).

## 9. Dependencies & alternatives

- DS deps: `Pc` tokens (colours, spacing, `pawnColor`), `PcText` (`amount`,
  `caption`), `Motion.stateFade`.
- Alternative considered: **take `GameSession` + seat index** and derive
  everything internally. Rejected - it would couple the DS to the app's central
  notifier and its l10n, and make the component untestable without a live
  session. Passing resolved strings keeps the boundary clean (the pattern the
  other domain composites should follow).

## 10. Testing

`test/design_components_test.dart` (SeatTile group): name+tags+figures render,
active shows the turn marker, rank 1 = crown / others = number, bankrupt =
strikethrough, lobby seat (no cash) shows no figures. Showcase: leader / idle /
bot / bankrupt / lobby + a narrow-width clip case. Load-bearing:
`test/layout_test.dart` (a loaded 6-seat panel must not overflow a Steam Deck).
