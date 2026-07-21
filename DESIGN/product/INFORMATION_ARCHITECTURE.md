# Information Architecture

The product-level model of **the information a player consults during a game,
and the requirements any interface must satisfy** to present it - independent of
any screen, layout, component, or implementation. Written so that a team (or an
AI) with no project history could reconstruct the same play experience.

> STATUS: **CONTRACT ONLY.** This file currently defines its scope and
> boundaries. The information content (inventory, permanent/contextual
> classification, consulted-together groups, priority) is pending contract
> validation and will be written in a later pass. Do not add detailed content
> until the contract below is confirmed.

## Scope

The information model of a live game: what information exists, when it becomes relevant, and which pieces are evaluated together, when, and
which pieces are read together - expressed as **requirements** an interface must
meet, never as an interface itself.

This document models product truth, not implementation. If implementation and this document disagree, the implementation is wrong or the product decision has changed.

## Responsibilities (what this document owns)

- The **inventory** of every piece of information the player consults during a
  game.
- The classification of each piece as **permanent** (must remain available
  throughout) vs **contextual** (belongs to a specific phase and then leaves).
- The **consulted-together groups**: which pieces of information the player
  reads in one thought (cognitive adjacency, not spatial adjacency).
- The **priority / salience** ordering, especially under time pressure, and the
  set that **must never be hidden**.

## Exclusions (what this document NEVER contains)

- No placement, geometry, spatial adjacency, layout, or any location.
- No screen-by-screen organization - the per-screen application is a separate,
  downstream concern.
- No components, widgets, styles, or motion.
- No behavioral narrative - what the player DOES and where they look is a
  separate, sibling concern; this file states the resulting information
  requirements only.
- No implementation detail of any kind.
