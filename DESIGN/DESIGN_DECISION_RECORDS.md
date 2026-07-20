# Design Decision Records (DDR)

The visual equivalent of the ADR process. A significant, contestable, or
non-obvious VISUAL/UX decision gets a DDR so the next designer inherits
the reasoning, not just the result - exactly as ADRs preserve
architectural WHY.

## When a DDR is required

- Any change to a rule in this bible (palette meaning, a new shape, a
  fourth typographic voice, a new attention device, a new component
  type, a new overlay layer).
- Any decision that a future designer might reasonably reverse without
  knowing why it was made (why no bounce? why no amber? why per-server
  rank framing?).
- Anything that touches a DESIGN/architecture boundary (chat, an
  achievement toast, a modal auction) - which is BOTH a DDR and,
  usually, an ADR.

A pure implementation of an already-decided rule needs no DDR. A style
tweak within the system (spacing, a new icon from the chosen set) needs
no DDR. When unsure whether it's significant, it is - write the DDR.

## Format (mirror the ADR house style - see AI_ENGINEERING.md)

File: `DESIGN/ddr/00NN-kebab-title.md`. Sections:

- **Context** - the design forces; cite the bible sections and any
  architectural constraint (ADR/INVARIANT) that bounds the decision.
- **Problem** - the specific question being answered.
- **Alternatives** - the options genuinely weighed, and why the losers
  lost. (Never invent alternatives you didn't consider - false history
  is worse than none, per the ADR guide.)
- **Decision** - what, precisely.
- **Trade-offs** - what it costs; what it forecloses.
- **Consequences** - what must change (tokens.dart, a bible section, a
  screen), what is deferred.
- **Review date** - when to revisit (visual decisions age faster than
  architectural ones; default 12-18 months, or "on the isometric board
  chantier", etc.).

Then update: the relevant bible file(s), and `lib/tokens.dart` /
`lib/motion.dart` if token values change. A DDR that ships without its
bible updates is incomplete (the X3 discipline).

## Seed record index (decisions already made, retro-numbered)

These predate the DDR process; they are recorded here so the reasoning
survives and future contestation has a target. Full reasoning lives in
the cited bible section / doc; this is the index.

| DDR | Decision | Where the reasoning lives | Revisit |
|---|---|---|---|
| DDR-001 | Art Deco flat register, sharp corners 0-2 px, no gradient/texture | ART_DIRECTION; visual-identity.md | on isometric board |
| DDR-002 | The validated `pc-*` palette + muted group colours + 6 pawn colours | COLOR_SYSTEM; visual-identity.md | with CVD audit |
| DDR-003 | Three type voices (Fraunces/Inter/Source Serif 4), bundled offline | TYPOGRAPHY; visual-identity.md | stable |
| DDR-004 | No third valence colour (no amber warning) | COLOR_SYSTEM law #1 | stable |
| DDR-005 | No bounce/spring anywhere; threat is the only asymmetric curve | motion-language 4.4 | stable (identity) |
| DDR-006 | Tabler + in-house glyphs, NOT mascot illustration | ICONOGRAPHY | on first styled screen |
| DDR-007 | Money travels as a chit; never a ticking delta | motion-language 4.2; GAME_FEEL | stable (identity) |
| DDR-008 | Gold-in-motion is reserved for victory points | motion-language 4.3; COLOR_SYSTEM law #2 | stable (identity) |
| DDR-009 | Fixed camera, always; attention via staging not motion | motion-language 2 | stable (identity) |
| DDR-010 | Tiered animation budget (8/6/4 s) compiled before play | motion-language 5; ADR-0030 | with any cap change |
| DDR-011 | Coach marks in the side panel, not floating over the board | DESIGN_SYSTEM; INVARIANTS C5 | stable |
| DDR-012 | Per-server rank framing in UI (no global-league promise) | PLAYER_EXPERIENCE; ADR-0034 | if global ladder lands |
| DDR-013 | No chat / no shop / no XP-passes (recorded non-goals) | DESIGN_PHILOSOPHY non-goals | requires reversal DDR+ADR |
| DDR-014 | Four audio-earcon categories; silence as default; no in-game music | AUDIO_DIRECTION | on audio pass (confirm) |
| DDR-015 | Speed stays the default; accessibility answered by configurability (untimed rooms) not slower defaults | ACCESSIBILITY | with a "relaxed" preset |

## Stability tiers (which decisions are load-bearing identity)

- **Identity - change only with extraordinary reason** (a reversal DDR
  that argues the brand should change): DDR-001, 004, 005, 007, 008,
  009, 013.
- **Firm - change with a normal DDR and playtest evidence**: DDR-002,
  003, 006, 010, 011, 012, 014.
- **Expected to evolve**: DDR-015 (relaxed preset), the CVD audit
  outcome, tier ladders, everything in COMMERCIAL_UX_AUDIT.
