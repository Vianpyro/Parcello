# Component inventory & build order

The complete set of components Parcello's UI needs, classified by
dependency level and ordered for implementation. This drives Phase 5-6 of
IMPLEMENTATION_ROADMAP.md. Component PHILOSOPHY (purpose/states/variants/
mistakes) lives in DESIGN_SYSTEM.md; this file is the LIST, the DEPENDENCY
ORDER, and the build tracker.

Rules for building down this list (per the owner's directive):

- **Dependency order**: never build a component before the ones it
  depends on. Levels below are a valid topological order.
- **One component per PR**, each **immediately reusable**, each
  **demonstrated in the Design Showcase** (`lib/ui/showcase/`, a
  debug-only screen). The showcase is where "reusable" is proven and
  where visual review happens.
- **Freeze the API as you go (DDR-0019)**: a component's constructor +
  its public props are public API the moment it lands - design them to
  last; later changes to that surface need a DDR. Internals stay free.
- **Adoption is incremental**: landing `PcButton` does NOT require
  migrating every `wideButton` in the same PR. The component + its
  showcase demo is the deliverable; call-site migration follows, small.
- **Value-preserving where it replaces something**: a component that
  supersedes an ad-hoc widget should be able to reproduce its look, so
  migration is pixel-safe (the spacing/typography discipline).

Status legend: TODO / BUILDING / DONE (frozen API).

## Level 0 - Foundations (DONE)

Tokens (`Pc`), typography (`PcText`), motion (`Motion`) - the design
system's public API base (DDR-0019). Not components; everything depends
on them.

## Level 1 - Core primitives (deps: L0 only)

Leaves first. These are the alphabet every screen and composite reuses.

**Order revised 2026-07** after the Connect Screen migration (the first
real-screen migration, DESIGN_FEEDBACK.md #1): the migration proved that
**inputs and dialogs - not decorative primitives - are what a real screen is
made of**. `PcTextField` and `PcDialog` are therefore promoted AHEAD of the
decorative trio (`PcHairline`/`PcChip`/`PcBadge`), which is deferred until the
blocking components exist. The `#` column keeps each component's original
identity; the **Build priority** column is the order we actually build in.
Rationale + findings: DESIGN_FEEDBACK.md action items A1/A2.

| # | Component | Purpose | Replaces | Build priority | Status |
|---|---|---|---|---|---|
| 1 | **PcButton** | the one button: primary/secondary/destructive/quiet, optional icon, hover earcon, disabled-with-reason | `wideButton`, ad-hoc Filled/Outlined/TextButton | 1 | DONE (frozen) |
| 2 | **PcCard** | dark surface container (radius, surface vs surface2, optional hairline border) - FLAT (no shadow); replacing a `Card` also drops its stray Material shadow (a flat correction) | scattered `Card(...)` | 2 | DONE (frozen) |
| 6 | **PcTextField** | themed single-line input (muted label, hairline underline, gold focus) | inline `TextField` (url/name/bid/comment) | **3 (promoted - A1)** | DONE (frozen) |
| 9 | **PcDialog** | confirm dialog: title + body + primary/cancel (L2, but promoted with PcTextField - Connect's sign-in needs both) | ad-hoc `AlertDialog` | **4 (promoted - A2)** | DONE (frozen) |
| 3 | **PcHairline** | a 1-2 px rule: neutral (`border`) or gold (`hairlineGold`) | raw `Divider`, inline hairlines | 5 (deferred) | TODO |
| 4 | **PcChip** | small dense tap-to-order/toggle chip (gold when selected) | route/mod bespoke `OutlinedButton`s | **built (pulled - blocked ActionsPanel + menu)** | DONE (frozen) |
| 5 | **PcBadge** | small status pill: spectator / bot / "you" / "unranked" | inline badge Rows | 7 (deferred) | TODO |

## Level 2 - Structural composites (deps: L1)

| # | Component | Purpose | Deps | Status |
|---|---|---|---|---|
| 7 | **PcPanel** | titled section = PcCard + title row + PcHairline | PcCard, PcHairline, PcText | TODO |
| 8 | **PcListRow** | leading / title / subtitle / trailing row | PcText (+ PcBadge) | TODO - sighted once (Settings label/value rows, DESIGN_FEEDBACK #2/D2); build on the 2nd consumer (lobby seat list) |
| 9 | **PcDialog** | confirm dialog: title + body + primary/cancel | PcButton | DONE (frozen) - built early (promoted, see L1 note) |
| 10 | **PcMarker** | persistent, dismissible marker card (AFK auto-play, connection, coach mark base) | PcCard, PcButton | TODO |

## Level 3 - Domain composites (deps: L1/L2 + engine view types)

| # | Component | Purpose | Deps | Status |
|---|---|---|---|---|
| 11 | **MoneyChit** | the parchment chit's STATIC presentation (`+/-amount`, sage/oxblood) - the travel is the director's | Pc, PcText.amount | TODO |
| 12 | **PropertyCard** | parchment face, group-colour band, rent ladder, mortgaged/conglomerate states | Pc, PcText, PcHairline | TODO |
| 13 | **SeatTile** (PlayerCard) | identity + cash + VP + connection/bot/acting state; the chit TARGET (fixed position) | Pc, PcText (NOT PcBadge/MoneyChit - see note) | DONE (frozen) |
| 14 | **TradeOfferCard** | an offer's give/receive + accept/refuse/cancel | PcCard, PcButton, PropertyCard | TODO |
| 15 | **SettingsField** | a labelled setting row (clamped input/chip) | PcListRow, PcTextField/PcChip | TODO |

## Level 4 - The hard one (deps: everything)

| # | Component | Purpose | Deps | Status |
|---|---|---|---|---|
| 16 | **AuctionWidget** | the anchored sealed-bid input + the tile-edge draining clock (motion-language 8.2, the #1 UX gap) | PcTextField, PcChip, PcButton + board/stage integration | TODO |

AuctionWidget is deliberately last: it needs the input, chip, and button
primitives proven first, it is the highest-risk surface (the 1024x600
floor, `bid_input_test`, the E5 masking invariant), and it is where the
whole system pays off.

## Build order (topological, one per PR)

```
[showcase scaffold]
  -> 1 PcButton -> 2 PcCard                          (L1, done)
  -> 6 PcTextField -> 9 PcDialog                     (promoted - real-screen blockers, A1/A2)
  -> 3 PcHairline -> 4 PcChip -> 5 PcBadge           (L1 decorative, deferred)
  -> 7 PcPanel -> 8 PcListRow -> 10 PcMarker         (rest of L2)
  -> 11 MoneyChit -> 12 PropertyCard -> 13 SeatTile -> 14 TradeOfferCard -> 15 SettingsField  (L3)
  -> 16 AuctionWidget                                                                    (L4)
```

Within a level the order is by reuse/impact and by what real screens
actually block on (the Connect migration promoted PcTextField/PcDialog
over the decorative trio - DESIGN_FEEDBACK.md), not a hard dependency. A
composite may only start once ALL its listed deps are DONE (frozen);
PcDialog depends only on PcButton, so its promotion is dependency-safe.

## Not components (do not build)

Chat (a moderation-surface ADR, not a UI drop-in - DESIGN_SYSTEM). A
generic "PcIcon" (Icon + the icon-set decision, DDR-0006, is enough).
Anything speculative: build a component when a real screen needs it, not
because the grid looks incomplete.

## Progress tracker

Update this in the SAME PR as the component lands.

| Component | PR / status | Frozen? | Showcase section |
|---|---|---|---|
| PcButton | DONE 2026-07 (`lib/design/components/pc_button.dart`) - **used in a real screen: Connect** | YES (DDR-0019) | Yes (PcButton) + `test/design_components_test.dart` |
| PcCard | DONE 2026-07 (`lib/design/components/pc_card.dart`) - **used in a real screen: Connect** | YES (DDR-0019) | Yes (PcCard: variants + narrow + text-zoom edge cases) + tests |
| PcTextField | DONE 2026-07 (`lib/design/components/pc_textfield.dart`) - **used in real screens: Connect** (url/name/issuer) **+ Settings** (dense numeric); grew `keyboardType`/`textAlign`/`dense`/optional-label additively for Settings (DESIGN_FEEDBACK #2/D1) | YES (DDR-0019; additive growth only) | Yes (PcTextField: empty/filled/counter + narrow + text-zoom + dense-numeric) + tests |
| PcDialog | DONE 2026-07 (`lib/design/components/pc_dialog.dart`) - **used in real screens: Connect** (sign-in) **+ Lobby** (resign); grew `destructive` additively for the resign confirm (DESIGN_FEEDBACK #3/D1) | YES (DDR-0019; additive growth only) | Yes (PcDialog: prompt + single-action + destructive) + tests |
| PcHairline | TODO (deferred) | - | - |
| PcChip | DONE 2026-07 (`lib/design/components/pc_chip.dart`) - **used in 2 real screens: ActionsPanel** (Legal Route builder) **+ menu** (mod picker) | YES (DDR-0019) | Yes (PcChip: idle/selected/disabled + tap-to-order) + tests |
| PcBadge | TODO (deferred) | - | - |
| PcPanel | TODO | - | - |
| PcListRow | TODO | - | - |
| PcMarker | TODO | - | - |
| MoneyChit | TODO | - | - |
| PropertyCard | TODO | - | - |
| SeatTile | DONE 2026-07 (`lib/design/components/seat_tile.dart`) - **used in the Game HUD / side panel** | YES (DDR-0019) | Yes (SeatTile: leader/idle/bot/bankrupt/lobby + narrow) + tests |
| TradeOfferCard | TODO | - | - |
| SettingsField | TODO | - | - |
| AuctionWidget | TODO | - | - |
