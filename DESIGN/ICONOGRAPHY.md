# Iconography

The open question in `docs/visual-identity.md` ("Tabler icons plus flat
in-house glyphs, instead of mascot illustration") is hereby resolved as
the direction - recorded as DDR-006, to be confirmed against the first
fully-styled screen like every art call.

## Philosophy

Icons are WAYFINDING, not decoration. Parcello's meaning-carriers are
already the four shapes (chit, band, chevron, rule) and the materials;
icons only label navigation and tools (menu tiles, panel actions,
toggles). Consequence: few icons, quiet icons, and NEVER an icon as
the sole carrier of a game-state meaning (game state uses the grammar,
not glyphs).

## Rules

- **Set**: Material outlined (current build) migrating to Tabler
  (MIT) at the audio/polish pass - both are acceptable interim; never
  mix two sets in one surface.
- **Style**: outlined, stroke ~1.5-2 px, squared joins/terminals where
  the set offers them (matches the hairline language). FILLED variants
  mean "state on" only (e.g. a filled eye = currently spectating) -
  never decorative preference.
- **Sizes**: 18 px inline with text, 24-28 px in menu tiles; snap to
  the 4-px grid; optical centering over mathematical.
- **Colour**: `pc-text-muted` at rest, `pc-gold` for the active/hover
  accent, pawn/valence colours NEVER (icons are chrome).
- **No emoji anywhere in product UI** (log, banners, buttons). ASCII
  discipline extends to the visual register.
- **No mascots, no illustrated characters** (DESIGN_PHILOSOPHY
  non-goal). Warmth comes from materials.

## Custom glyphs (when the set lacks a concept)

Draw in the house geometry: built from straight strokes, steps, and
45-degree cuts; 2 px stroke on a 24 grid; must sit unnoticed in a row
of set icons. Candidates that will eventually need one: the chit, the
chevron/VP mark, a conglomerate cap, the Exposition. Keep them in one
`assets/icons/` sheet with licence notes, mirrored per the fonts
precedent (SHA256SUMS + README).

## Common mistakes

Icon-only buttons without labels (always pair at panel scale; tooltips
are desktop-only and controllers have none); introducing a rounded-
friendly icon set (breaks the squared language); using oxblood icons
for non-threats; animating icons (icons are still; the grammar moves).
