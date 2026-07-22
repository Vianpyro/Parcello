# DDR-0021: the in-game screen adopts a fixed five-region layout

Status: DECIDED (owner, 2026-07)

## Context

The validated global mockup (2026-07) reshapes the in-game screen. The
current `lib/ui/game/game_screen.dart` is a two-region split - a board on
the left, one scrolling 340px side panel on the right, with the status
line / clocks / contextual actions / event log stacked inside the board's
centre (`CenterPanel`). SCREEN_ARCHITECTURE's Game section describes that
layout. The mockup instead distributes the HUD around the board.

## Problem

What is the durable spatial structure of the in-game screen, so every
later restyle and the isometric board (DDR-0022) build against one region
model rather than re-inventing placement per change?

## Alternatives

1. **Keep the two-region split, restyle only.** Pros: least churn. Cons:
   does not match the mockup; leaves the HUD crowding the board centre
   (the 1024x600 floor problem SCREEN_ARCHITECTURE already flags) instead
   of moving it out to dedicated regions.
2. **Five-region model (decided).** Move the HUD out of the board centre
   into stable regions the mockup defines. Pros: matches the target;
   frees the board centre; each region owns one information group.
   Cons: a real reflow of `game_screen.dart` and the panels.
3. **Persistent action-panel row** (all of Carte/Enchere/Construire/
   Echanger visible at once). Rejected: contradicts SCREEN_ARCHITECTURE's
   "one primary action, contextual, singular" (owner-confirmed); the
   mockup's bottom row is the design SPEC of each contextual state, not
   four panels shown simultaneously.

## Decision

The in-game screen is **five stable regions**, spatial constancy preserved
(no region moves on a state change):

- **Top - player bar**: the acting/self player plus every seat (name, cash,
  VP), the game clock. Replaces the seat list that lived in the side panel.
- **Left - nav rail**: Menu, Objectives, History, Chat (vertical).
- **Centre - board** + the "your turn" prompt + the **single contextual
  action panel** (bid / build / trade / jail exit / end turn, by phase -
  the model is unchanged, only restyled and re-placed).
- **Bottom-left - the hand**: movement-card selection ("Vos cartes
  deplacement").
- **Right - property panel** (landed/selected tile: name, rent ladder,
  owner, houses/hotels) **and** the history feed.

## Trade-offs

- A real reflow of `game_screen.dart`, `center_panel.dart`,
  `actions_panel.dart`, `side_panel.dart` (seats leave it). Contained: the
  data all already exists on `GameSession`; this moves and restyles it.
- The `bid_input` invariant (the contextual panel built once and handed to
  the board as a `child`) MUST survive the reflow (`bid_input_test.dart`).
- The 1024x600 floor is re-opened: moving the HUD OUT of the board centre
  should help, but the denser regions must be re-measured (`layout_test`).

## Consequences

- SCREEN_ARCHITECTURE.md Game section is rewritten to this region model
  (in the same change set).
- Some regions carry placeholder features (avatars, level/XP, chat,
  objectives) - governed by DDR-0023; per-server rank framing stays
  DDR-0012.
- Delivered flat-board first (DDR-0022 phases the isometric board after
  this reflow), so the layout ships playable before the risky renderer.

## Review date

After the first playtest on the new layout, or if the region model fights
the isometric board (DDR-0022).
