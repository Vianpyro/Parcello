# DDR-0022: the board goes isometric (reverses DDR-0017)

Status: DECIDED (owner, 2026-07). **Reverses DDR-0017.**

## Context

DDR-0017 kept the board flat "for now" and deferred the isometric board to
"its own future chantier with its own DDR" - this is that DDR. The
aspirational isometric "city of progress" is specified in
`docs/visual-identity.md` (S146-197) and `DESIGN/ART_DIRECTION.md` (S90-98):
diamond projection, buildings as stepped flat silhouettes in group colours
with gold caps. The owner has decided (2026-07) to build it now, as the
centre of the game-screen refonte (DDR-0021).

The enabling guarantee, established by DDR-0017 and motion-language S14:
the motion layer is **projection-independent**. Anchors are abstract
(`TileAnchor`/`SeatAnchor` -> `AnchorRegistry`, `lib/stage.dart`); the ONLY
tile-index-to-screen-point mapping is the resolver keyed on `_ringKey` /
`_tileCenter` in `lib/board.dart`. So the projection is a **re-anchoring**
of that one resolver plus the tile rendering, NOT a rewrite of motion.

## Problem

Adopt the isometric board, and do it without destabilising the animation
system, hit-testing, accessibility, or the shipping-size floor.

## Alternatives

1. **Stay flat** (DDR-0017's "for now"). Rejected: the owner wants the
   aspirational look now; the flat board was always explicitly interim.
2. **Isometric first, layout after.** Rejected: front-loads the riskiest
   work; DDR-0021's reflow can ship playable on the flat board first.
3. **Isometric after the flat reflow (decided).** The layout (DDR-0021)
   ships on the flat board; the isometric renderer is a dedicated later
   phase behind the already-projection-safe motion layer.

## Decision

Replace the flat ring renderer with a **diamond isometric** projection for
the `4*(d-1)` ring boards, sequenced AFTER the DDR-0021 flat-board reflow.
Scope:

- **Projection**: tile index -> diamond cell -> screen point.
- **Buildings**: stepped flat silhouettes in the group colour with gold
  caps (houses; the top level reads as a "hotel", per DDR-0023's visual
  mapping of the single build ladder).
- **Re-anchoring**: the `board.dart` resolver returns ISO points, so chits,
  overlay, and every director beat keep working unchanged. `SeatAnchor`
  now targets the top player bar (DDR-0021).
- **Hit-testing + accessibility**: tiles stay tappable AND D-pad focusable
  (`FocusableActionDetector`, gold focus ring) in projected space.
- **Fallback**: non-ring mod boards keep the existing flat wrap layout.

## Trade-offs

- The single highest-risk visual chantier (projection, silhouette system,
  hit-testing in projected space, perf). Isolated to the board renderer
  and the one resolver, precisely because the motion layer survives.
- Perf: an explicit 60fps target; iso rendering cost must be measured.
- The 1024x600 floor may move; re-measure with the iso board
  (`layout_test`, SCREEN_ARCHITECTURE).

## Consequences

- DDR-0017 is REVERSED (its index row and file are marked so).
- IMPLEMENTATION_ROADMAP.md: the isometric board leaves "out of core path".
- `docs/visual-identity.md` / `ART_DIRECTION.md` iso specs become the build
  reference (no restating here).
- Motion-language S14's origin->destination anchor rule is now load-bearing
  in practice, not just in principle - do not add a coordinate-baked beat.

## Review date

After the iso renderer ships and a perf + accessibility pass on real
hardware (desktop + Steam Deck).
