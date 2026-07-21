# Component Architecture Records (CAR)

A **CAR** is a short architecture document for ONE design-system component,
written and ratified **before that component is implemented**. It is the
component-scoped sibling of the two existing record types:

- **ADR** (`docs/adr/`) - server / engine / protocol architecture.
- **DDR** (`DESIGN/ddr/`) - cross-cutting VISUAL / UX decisions (palette,
  typography rules, the in-tree design-system boundary...).
- **CAR** (`DESIGN/car/`) - the architecture of a SINGLE component: its
  responsibility, boundaries, public API, invariants, and how it meets Motion,
  Accessibility, and Localization.

A CAR may cite ADRs and DDRs; it never contradicts them (DDR-0019 governs the
API-stability contract that every CAR's "Public API" section inherits).

## When a CAR is REQUIRED (the gate)

**Every L3/L4 domain composite requires a ratified CAR before a line of it is
written** (owner rule, 2026-07): `SeatTile`, `PropertyCard`, `MoneyChit`,
`TradeOfferCard`, `SettingsField`, `AuctionWidget`, and any later domain
component. These bind engine VIEW types, the stage/Motion layer, and localized
content - they have real architecture surface, and getting their boundaries
wrong is expensive (they are frozen API once shipped, DDR-0019).

**L0-L2 primitives do NOT need a CAR** (`PcButton`, `PcCard`, `PcText`,
`PcTextField`, `PcDialog`, `PcChip`, `PcHairline`, `PcBadge`, `PcPanel`,
`PcListRow`, `PcMarker`). They are small, presentation-only, and already
governed by the inventory + the DDR-0019 freeze. Writing a CAR for one is
allowed but not required.

Pull-based selection is unchanged: it still decides WHICH component is built
next (the one whose absence blocks the next real screen). The CAR is the gate
that must clear **between** "this component is next" and "start coding it".

## Process

1. A real screen's migration is blocked by a missing L3/L4 component (pull-based).
2. **Write its CAR** from the template below. Fill every section; "N/A - why"
   is an acceptable answer, a blank is not.
3. **Ratify** (owner review, or an equivalent written justification in the same
   change - mirrors DDR-0019's allowance). Status moves DRAFT -> RATIFIED.
4. **Only then implement**, following the CAR. Showcase + tests + inventory
   update as usual.
5. **Keep the CAR in sync**: additive API growth (a new defaulted param, per
   DDR-0019) is a one-line amendment to the CAR's Public API + Extensibility
   sections in the same PR. A change to responsibility, boundaries, or an
   invariant needs a NEW CAR (or a superseding revision), never a silent edit.

## Template

```
# CAR-XXXX: <ComponentName>

Status: DRAFT | RATIFIED (owner, YYYY-MM) | SUPERSEDED by CAR-YYYY
Level: L3 | L4    Inventory: #NN    Pulled by: <the screen that blocked on it>

## 1. Responsibility
One sentence: the single thing this component is responsible for. Then the
"it does NOT" list - responsibilities that look adjacent but belong elsewhere.

## 2. Boundaries
- Layer: where it sits; what it may import (view types? never session/l10n?).
- Inputs: what data shape it takes and why (formatted strings vs raw + why).
- Ownership: what the PARENT computes/formats vs what the component owns.

## 3. Public API
The constructor + every named param, with type and meaning. This is frozen
API the moment it ships (DDR-0019). Note which params are required.

## 4. Invariants
Must-always / must-never for this component. Each should be checkable (a test,
or a review rule).

## 5. Extensibility
How it is expected to grow (additive defaulted params - list the anticipated
ones and their trigger). What change would instead require a new CAR/DDR.

## 6. Motion
Which Motion tokens/tiers it uses (never a raw Duration). Whether it is a
director beat, an implicit transition, or an animation TARGET (anchor) only.
Reduced-motion behaviour.

## 7. Accessibility
Focus/traversal (or explicitly non-interactive). Never colour-only signalling
(the redundant channel). Text scaling / narrow-width behaviour. Semantics.

## 8. Localization
What it receives already-localized (INVARIANTS C1: no literals, no formatting
of numbers/plurals inside the component unless it owns that presentation).

## 9. Dependencies & alternatives
DS deps (tokens, PcText, other components). One line on the main alternative
considered and why it lost.

## 10. Testing
The unit tests that prove the invariants + the showcase states.
```

## Index

| CAR | Component | Level | Status | Pulled by |
|---|---|---|---|---|
| [0001](0001-seat-tile.md) | SeatTile | L3 | RATIFIED (retroactive) | Game HUD / side panel |

Future domain components append here the PR their CAR is ratified, before code.
