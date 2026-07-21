# The Parcello Design Bible

The visual and experiential equivalent of the Technical Bible: everything
a future designer - human or AI - needs to build new Parcello surfaces
WITHOUT inventing new styles. Written 2026-07 by the project's departing
creative direction, to remain useful for a decade.

## Precedence (absolute - design adapts to architecture, never the reverse)

```
docs/architecture.typ  >  docs/adr/*  >  docs/INVARIANTS.md
        >  docs/visual-identity.md  +  docs/motion-language.md   (CANONICAL design sources)
        >  DESIGN/*                                              (this bible)
        >  implementation (lib/tokens.dart, lib/motion.dart, ...)
```

Two documents predate this bible and REMAIN canonical for their domains -
this bible builds on them and never restates their tables:

- **`docs/visual-identity.md`** - the validated palette (hex values live
  there and in `lib/tokens.dart`, nowhere else), group/pawn colours,
  fonts, the board and menu content specs.
- **`docs/motion-language.md`** - the motion doctrine, tiers, grammar,
  budget, the full 43-event catalogue, motion profiles. It is binding on
  client code via ADR-0030.

If anything in DESIGN/ appears to contradict them or an ADR, the older
canon wins and DESIGN/ has a bug - fix it via a DDR
(DESIGN_DECISION_RECORDS.md).

## The map

| File | Question it answers |
|---|---|
| DESIGN_PHILOSOPHY.md | why does Parcello look and feel like ANYTHING? |
| CONCEPT_CRITIQUE.md | how to digest inspiration without swallowing it (worked example) |
| PLAYER_EXPERIENCE.md | what should the player FEEL, phase by phase? |
| ART_DIRECTION.md | what is in-register and what is forbidden? |
| VISUAL_LANGUAGE.md | geometry: grid, spacing, radius, elevation, layering |
| COLOR_SYSTEM.md | what does each colour MEAN (not what it is) |
| TYPOGRAPHY.md | families, roles, numbers, hierarchy |
| ICONOGRAPHY.md | glyph rules |
| MOTION_GUIDELINES.md | the ten motion laws (digest; canon = motion-language.md) |
| GAME_FEEL.md | complete feedback loops for the actions that matter |
| AUDIO_DIRECTION.md | the sound identity (earcons, music, silence) |
| DESIGN_SYSTEM.md | component-by-component philosophy |
| SCREEN_ARCHITECTURE.md | per-screen rules (not layouts) |
| UX_GUIDELINES.md | cognitive load, feedback, inputs, empty/error states |
| ACCESSIBILITY.md | the non-negotiables |
| DESIGN_DECISION_RECORDS.md | the DDR process + the record index; full records in `ddr/` |
| car/README.md | the CAR process (Component Architecture Records): every L3/L4 domain component needs a ratified CAR in `car/` BEFORE implementation |
| COMPONENT_INVENTORY.md | the component list, dependency order, build tracker + the CAR gate for L3/L4 |
| DESIGN_FEEDBACK.md | what each real-screen migration taught us; the living Design-System / Gameplay / Visual-Debt coverage registers |
| DESIGN_REVIEW.md | how to review any UI change |
| IMPLEMENTATION_ROADMAP.md | the Flutter BUILD ORDER (foundations->components->screens->polish) + the maintained built-vs-not-built truth |
| COMMERCIAL_UX_AUDIT.md | what is missing, prioritized by player impact |
| SELF_CRITIQUE.md | where this bible is weak; what may evolve vs must not |

## How to use it

Building a screen: PHILOSOPHY once, then SCREEN_ARCHITECTURE +
DESIGN_SYSTEM + COLOR/TYPOGRAPHY as you work, DESIGN_REVIEW before you
ship. Adding motion or sound: motion-language.md first, then
MOTION_GUIDELINES/AUDIO_DIRECTION. Changing the look of anything:
check DDR index - it may be a recorded decision; amend via a new DDR,
never silently. Building an L3/L4 domain component (SeatTile, PropertyCard,
MoneyChit, TradeOfferCard, AuctionWidget, ...): write and ratify its CAR
(`car/`) FIRST - implementation does not start before the record.

Update discipline is the technical repo's X3 invariant applied here:
these are living documents; a shipped visual change that contradicts
them must update them in the same change, or the change is incomplete.
