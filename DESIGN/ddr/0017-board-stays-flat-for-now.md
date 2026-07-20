# DDR-0017: the board stays flat for now; isometric is deferred

Status: DECIDED (owner, 2026-07)

## Context

`docs/visual-identity.md` and ART_DIRECTION.md name an isometric "city of
progress" board as the long-term artistic destination. The current
`lib/board.dart` renders a flat ring. The design-application work
(IMPLEMENTATION_ROADMAP) needs to know whether to build the design system
and the auction moment against the flat board or wait for the isometric
one - this gates the single biggest UX gap (the anchored auction input,
Phase 6).

## Problem

Do we invest the design-system and auction-anchor work in the flat board
now, or hold it for an isometric board first?

## Alternatives considered

1. **Isometric first**. Pros: reach the aspirational look sooner; avoid
   anchoring the auction to a flat tile that a projection will move. Cons:
   a large, high-risk chantier (diamond projection, building silhouette
   system, hit-testing) that would BLOCK all design-application work for
   weeks; more bugs, more debt, harder to test; and it front-loads the
   riskiest visual work before the system that should underpin it exists.
2. **Flat now, isometric later** (decided). Pros: unblocks the entire
   roadmap immediately; fewer bugs; faster; less debt; easier to test;
   the design system is projection-INDEPENDENT, and the motion primitives
   are already defined as origin->destination on board objects
   (motion-language 14) so they SURVIVE a later projection change. Cons:
   the auction anchor built on flat tiles will need revisiting when the
   board goes isometric (bounded - it is re-anchoring to the same tile
   objects, not a redesign).

## Decision

**Flat board for now.** All design-system and gameplay-polish work
(including the anchored auction input, Phase 6) targets the flat board.
The isometric board is a separate future chantier with its own DDR when
it starts.

## Trade-offs

- We accept re-anchoring the auction input (and any other tile-anchored
  UI) once, later, when the board goes isometric - a contained cost -
  in exchange for unblocking all design work now and keeping risk low.
- The aspirational look waits. Acceptable: the flat board already carries
  the full palette, materials, and motion grammar; it is not a
  placeholder, it is a legitimate register (the vintage-poster flatness
  is on-brand, ART_DIRECTION).

## Consequences

- IMPLEMENTATION_ROADMAP treats the isometric board as out of its core
  critical path.
- Every motion primitive stays defined as origin->destination on board
  objects (already true) so nothing built now depends on the board being
  flat - the guarantee that makes the eventual projection a
  re-anchoring, not a rewrite.
- When the isometric chantier is proposed, it gets its own DDR covering
  projection, building silhouettes, hit-testing, and the re-anchoring of
  tile-anchored UI.

## Review date

When the isometric board is seriously proposed (post-1.0, per the product
roadmap) - not before.
