# Visual language - geometry and space

The measurable half of the art direction. Values here are the working
system extracted from the built client (`lib/tokens.dart`,
`lib/ui/menu/geometry.dart`); token VALUES live in code and
visual-identity.md - this file defines the RULES for using them.

## Grid & spacing

- Base unit **4 px**; standard steps 4 / 8 / 12 / 16 / 24. Panel
  padding 12; card padding 12-24; gaps between siblings 6-12.
- Content max-widths, not fluid sprawl: menus center at ~680 px;
  dialogs/cards ~380-460 px. The game screen is a fixed composition:
  board (flexible) + 340 px side panel + 12 px gutters.
- Density rule: the SIDE PANEL absorbs growth by scrolling; the board
  never scrolls, the page never scrolls horizontally. Anything that
  can grow with the room (trades, log, hints) lives in the scrolling
  panel - this is also a layout-test invariant (INVARIANTS C5).

## Corner radius

0-2 px, everywhere, no exceptions (`Pc.radius` = 2). Buttons, cards,
dialogs, chips, text fields. A rounded element is off-brand at any
radius above 2. (Focus rings follow the same geometry.)

## Elevation & layering

Colour-stepped, not shadow-stepped:

| Level | Token | Use |
|---|---|---|
| 0 | `pc-bg` | app background |
| 1 | `pc-surface` | cards, tiles' dark variants, panels |
| 2 | `pc-surface-2` | dialogs, hover, raised chips, badges |
| lift | +2 px offset, `Pc.hairShadow` | "act on this" only (P2) |

Z-order on the game screen (fixed): board < centre HUD < side panel <
travelling overlay (chits/chevrons/arrest) < coach marks & toasts <
dialogs. Nothing else may claim a layer; new overlay needs a DDR.

## Borders & shadows

- Hairlines only: 1 px `pc-border` (structure), 1-2 px `pc-gold`
  (subject/focus/time), dashed `pc-border-muted` (coming-soon).
- Double-rule framing for ceremonial surfaces (end screen, wordmark).
- The ONLY shadow in the product is `Pc.hairShadow` under a lifted
  element. No soft halos, no glows, no elevation shadows on cards.

## Glass, blur, transparency

Banned (see ART_DIRECTION texture rules), with exactly two sanctioned
opacity uses: **recede** (35% - attention device) and the disconnect
grey (board at 80% while the socket is down). Opacity means "less
present", never "frosted".

## Backgrounds

`pc-bg` flat. No imagery behind UI, no vignette. The future isometric
board's surroundings are part of the DRAWN world (its own DDR when the
chantier starts), not a UI background.

## Surface hierarchy in practice

dark chrome = machine; parchment = play object; gold = value/subject
(scarce). A component whose material doesn't answer "machine, object,
or value?" in one glance is wearing the wrong clothes - re-read
ART_DIRECTION's contrast section.

## Visual rhythm

The game breathes at 12 seconds (a turn). Layout should echo that
economy: one primary action zone per screen state, steady positions
for recurring elements (banner center-top of board, clock on the
subject tile, log bottom of centre HUD), and NO layout shifts on state
change - state changes restyle elements in place (band colours, pips,
hairlines), they do not reflow the page. Reflow is the visual
equivalent of a camera move, and the camera never moves.
