# Art direction

Canonical base: `docs/visual-identity.md` (palette values, fonts, board
and menu content specs live THERE). This file adds the register's
boundaries: what belongs, what is banned, and the physics of the look.

## The register in one sentence

**Art Deco "city of progress", rendered as a flat vintage travel
poster, lit by its own gold.** Quietly opulent; geometry does the
ornament's job; restraint reads as quality.

## Shape language

- **Squared, stepped, symmetric.** Radius 0-2 px everywhere. Tiering
  (ziggurat steps) is the house motif - it appears in the chevron (VP),
  in future building silhouettes, in frame corners. When you need an
  ornament, step a line; never curl one.
- **Hairlines are structure.** The 1-2 px rule (gold on dark, ink on
  parchment) frames, separates, and - as a drain - tells time. Double
  rules for emphasis; never thick bars.
- **Paper is an object.** Parchment elements (tiles, chits, cards,
  receipts) are card-stock: crisp edge, ink text, colour as an EDGE
  BAND never a fill. Paper is the play surface; dark is the chrome.
- **Chevrons point, bands own, chits pay, rules frame** - the four
  shapes of motion-language 4.3. New shapes need a DDR.

## Lighting philosophy

There are no light sources. Nothing casts (except the 2 px hard lift
shadow, which is a diagram of elevation, not light). "Glow" is banned;
gold looks lit because it sits on `pc-bg` at high contrast, not because
of bloom. This is identity AND engineering: flat colour renders
identically across canvas/web/Deck and never ages the way rendered
lighting does.

## Contrast philosophy

Two worlds, deliberately opposed: **dark chrome** (bg/surface/borders -
where the machine lives: menus, panels, logs) and **light paper**
(parchment - where the GAME lives: tiles, chits, cards). The eye learns
that light = play object, dark = interface. Guard this: a parchment
settings panel or a dark property card each blur the boundary that
makes the board scannable. Gold belongs to both worlds and bridges
them - which is exactly why it must stay scarce (COLOR_SYSTEM).

## Depth philosophy

Depth is a STATEMENT, not a style. Three planes only: recede (35%
opacity - "not now"), base, lift (+2 px, +2% - "act here"). Elevation
between surfaces is one token step (`bg -> surface -> surface2`),
expressed by colour, not shadow. The future isometric board adds
PROJECTED depth (a drawn world) without changing UI depth rules: HUD
and panels stay flat over it.

## Texture philosophy

None. Parchment is a COLOUR, not a paper scan; dark surfaces are flat,
not brushed metal. Grain, noise, vignettes, and blur are all banned -
they are the fastest way to age a UI by five years and to break canvas
rendering consistency.

## Acceptable inspirations (and what to take)

- **Arcane's Piltover** - geometry, stepped silhouettes, brass-on-dark
  mood. SHAPES only; never hextech iconography or any protected asset.
- **Vintage travel posters / WPA prints** - flat planes, confident
  typography, limited palettes.
- **Into the Breach** - board discipline, threats drawn on targets.
- **Mini Motorways / Dorfromantik** - restraint as quality; tiny
  vocabularies.
- **Classic bank/ledger ephemera** - receipts, stamps, tabular money.

## Forbidden inspirations (the drift catalogue - check every mockup)

- Monopoly / Business Tour TRADE DRESS: no lookalike mascots, token
  shapes, board fonts, or corner art. Layout lessons only (recorded
  stance in visual-identity.md). This is a legal boundary, not taste.
- Casino/Vegas: jackpot counters, coin rain, marquee bulbs, red-gold
  velvet. (Adjacent to our palette - which is why it's listed; the
  difference is restraint.)
- Neon cyberpunk / glassmorphism / aurora gradients: the 2020s default
  drift; one glow undoes the register.
- Mobile-F2P juice: bounce, sparkle trails, chest-opening suspense.
- Photorealistic tabletop: felt, wood grain, dice-cup skeuomorphs
  (also: no dice exist - ADR-0017).
- Corporate flat-illustration people (the "alegria" style): wrong
  warmth, wrong decade.

## The future isometric board

The one large aspirational piece (visual-identity.md board spec + the
concept image's building silhouettes as reference): diamond projection,
buildings as stepped flat silhouettes in group colours with gold caps
at conglomerate level, centre plaza in sage. Non-negotiables carried
into it: fixed camera, flat shading, tiles remain parchment cards,
every motion primitive already defined as origin->destination survives
the projection (motion-language section 14 was written to guarantee
this).
