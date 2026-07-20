# Self-critique

A principal designer's honest review of this bible, written by its
author. The purpose is to hand the next designer the doubts, not just
the doctrine - the same discipline the technical LEGACY.md applies to
itself.

## Where this bible is still weak

- **It is a bible for a game that hasn't been felt yet.** Almost every
  emotional claim (PLAYER_EXPERIENCE, GAME_FEEL) is a HYPOTHESIS -
  Parcello has never had a real multi-human playtest. The doctrine is
  internally coherent and grounded in the mechanics, but "the player
  should feel X" is unverified until strangers play. Treat the feeling
  claims as testable predictions, not established facts.
- **The auction is the whole game and the least built.** The bible
  describes the anchored-to-tile sealed-bid experience in loving detail
  (DESIGN_SYSTEM, GAME_FEEL, motion-language 8.2) and it does not yet
  exist. The most important document section points at the biggest hole.
  That is honest but uncomfortable: the identity moment is spec, not
  screen.
- **No pixel specs, by choice - which cuts both ways.** I documented
  rules, not layouts, because layouts rot and rules don't. But a future
  implementer gets no measured spacing, no component redlines, no
  Figma. For a solo/AI-built project that is right (the code IS the
  layout, tokens.dart IS the spec); for a design team it would be
  under-specified. If a team ever forms, the missing artifact is a
  living component gallery (a "widgetbook"), not more prose.
- **The isometric board is hand-waved.** It is the stated artistic
  destination and I gave it a paragraph. The projection math, the
  building silhouette system, hit-testing, and how the flat motion
  primitives actually land on a diamond board are all unaddressed. That
  chantier needs its own design doc when it starts (a DDR at minimum).
- **Iconography is the least-resolved system.** DDR-006 picks a
  direction (Tabler + in-house) but no icon has been drawn in the house
  geometry; the "custom glyph" rules are untested against real glyphs.
- **Audio is a brief, not a direction.** I can describe the register in
  words (dry, brass-and-paper, four earcons) but audio identity is made
  by ears, not prose. A composer/sound designer will need to interpret,
  and the bible can't validate their output - only the four-category
  discipline is enforceable.

## Uncertain decisions (I could be wrong)

- **No in-game music at all.** Defensible (12 s decisions, voice-chat
  context, earcon clarity) but it's a strong claim; some players read
  silence as unfinished, not composed. A quiet ambient bed might test
  better than I expect. Left as DDR-014, "confirm on the audio pass."
- **Per-server rank framing forever.** Correct given the architecture
  today, but if a global ladder ever ships (signed results), the whole
  rank/profile emotional design shifts, and PLAYER_EXPERIENCE's "club
  reputation" framing may feel small. Flagged, not resolved.
- **Speed as identity vs accessibility.** I chose "keep the default
  fast, answer accessibility with configurability" (DDR-015). That's a
  values call that trades away some inclusivity for identity. A "relaxed
  preset" softens it but doesn't erase the tension. Watch it in
  playtests with older/casual groups.
- **The no-third-valence rule (no amber).** Elegant, and it protects
  gold - but real UIs often want a "caution, not danger" state. I
  forced everything into gain/loss/neutral; if that proves too rigid in
  practice (e.g. a "your offer expired" that is neither), the honest fix
  is a DDR that adds one carefully-scoped neutral-attention treatment,
  NOT sneaking amber in.

## Future research topics

- Playtest instrumentation for FEELING, not just balance: can the
  post-game survey capture "did the auction feel tense / fair / clear?"
- CVD simulation of the full palette (group + pawn + gold together) -
  a concrete audit, not a vibe.
- Screen-reader narration quality: the log is textual, but is a
  turn-by-turn log actually usable as the primary channel, or does a
  blind player need a different summarization?
- Whether coach marks (five, contextual) are the right onboarding, or
  whether the "watch a bots game" path should be promoted to the
  primary teacher.

## Possible future redesigns (name them so they're not surprises)

- The board going isometric (the big one).
- A rank/profile system if the global ladder lands.
- A first-run experience richer than coach marks (guided vs-bots game)
  if playtests show the current onboarding underserves total newcomers.
- A component gallery / design-tokens site if a team forms.

## What should be STABLE for ten years vs EVOLVE

**Stable (the identity - reversing any of these is a different game's
design, and needs an extraordinary DDR):** the flat Art Deco register;
no bounce; the four-shape motion grammar; gold-in-motion = VP; money
travels as a chit; the fixed camera; the board-is-protagonist /
HUD-is-receipt split; dark-machine vs parchment-play-object; readability
> beauty > truth ordering; the no-dark-patterns non-goals.

**Expected to evolve (and SHOULD):** every specific screen layout; the
icon set; the exact audio; tier ladders and rank presentation; the
onboarding depth; the accessibility profiles (high-contrast, relaxed,
screen-reader); the whole COMMERCIAL_UX_AUDIT; the board's dimensional
rendering. These are the surface; the stable list is the skeleton.

## The one thing to protect above all

If a future contributor internalizes only one sentence: **the interface
must never show what the game is hiding, never imply what the server
did not say, and never move the camera.** Those three are where design
meets architecture, and where a well-meaning "improvement" does the most
damage. Everything else in this bible is negotiable through a DDR; those
three are the wall.
