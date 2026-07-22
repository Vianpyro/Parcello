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
| DDR-006 | Tabler + in-house glyphs, NOT mascot illustration (owner-confirmed 2026-07) | ICONOGRAPHY | on first styled screen |
| DDR-007 | Money travels as a chit; never a ticking delta | motion-language 4.2; GAME_FEEL | stable (identity) |
| DDR-008 | Gold-in-motion is reserved for victory points | motion-language 4.3; COLOR_SYSTEM law #2 | stable (identity) |
| DDR-009 | Fixed camera, always; attention via staging not motion | motion-language 2 | stable (identity) |
| DDR-010 | Tiered animation budget (8/6/4 s) compiled before play | motion-language 5; ADR-0030 | with any cap change |
| DDR-011 | Coach marks in the side panel, not floating over the board | DESIGN_SYSTEM; INVARIANTS C5 | stable |
| DDR-012 | Per-server rank framing in UI (no global-league promise) | PLAYER_EXPERIENCE; ADR-0034 | if global ladder lands |
| DDR-013 | ~~No chat / no shop / no XP-passes~~ **REVERSED by DDR-0023** (chat/shop/levels-XP enter scope as placeholders, owner 2026-07) | DESIGN_PHILOSOPHY non-goals; [ddr/0023](ddr/0023-reverse-lean-nongoals.md) | when any becomes real (its ADR) |
| DDR-014 | Four audio-earcon categories; silence as default; no in-game music; audio pass DEFERRED until the events exist (owner-confirmed 2026-07) | AUDIO_DIRECTION | on audio pass (confirm sound identity) |
| DDR-015 | Speed stays the default; accessibility answered by configurability (untimed rooms) not slower defaults | ACCESSIBILITY | with a "relaxed" preset |
| **DDR-016** | Design system in-tree (`lib/design/`) FOR NOW; package extraction DEFERRED not refused, gated on explicit criteria (2nd consumer / replay viewer / companion app / DS stabilization / boundary not holding) - owner-accepted 2026-07 | [ddr/0016](ddr/0016-design-system-in-tree-not-package.md) | when an extraction criterion fires (else 12-mo backstop) |
| **DDR-017** | ~~Board stays flat for now~~ **REVERSED by DDR-0022** (isometric board adopted, owner 2026-07) | [ddr/0017](ddr/0017-board-stays-flat-for-now.md); [ddr/0022](ddr/0022-isometric-board.md) | after iso ships + perf/a11y pass |
| **DDR-018** | Typography roles carry size+weight+family + a DEFAULT colour (overridable); some omit size to inherit it - the Phase 2 taxonomy | [ddr/0018](ddr/0018-typography-roles-carry-default-colour.md) | at high typography coverage, or if the default-colour ergonomics prove wrong |
| **DDR-019** | The design system's PUBLIC API (`Pc`/`PcText`/`Motion`/components) is a stability contract: internals free, additions free, but renames/removals/semantic changes need a DDR or in-diff justification (owner-set 2026-07) | [ddr/0019](ddr/0019-design-system-public-api-is-a-stability-contract.md) | on package extraction (DDR-0016), when a real semver policy replaces it |
| **DDR-020** | L3/L4 components take one immutable **Semantic Model** (engine-free, pre-localized, ZERO rendering info - skins are a goal) + framework-owned transient state (focus/hover/MediaQuery/ticker) + explicit intents; the mandatory `Presentation`-object proposal was rejected as fighting Flutter (owner-ratified 2026-07) | [ddr/0020](ddr/0020-component-data-flow.md) | if a skin system or a reactive framework change reopens the model boundary |
| **DDR-021** | In-game screen adopts a fixed five-region layout (player bar / nav rail / board+contextual panel / hand / property+history); "one primary action" kept; a persistent all-panels row was rejected (owner 2026-07) | [ddr/0021](ddr/0021-game-screen-region-layout.md); SCREEN_ARCHITECTURE | after first playtest on the new layout |
| **DDR-022** | Board goes isometric (diamond projection, stepped group-colour silhouettes, gold caps), sequenced after the flat reflow; re-anchors the one tile->screen resolver so motion survives - REVERSES DDR-017 (owner 2026-07) | [ddr/0022](ddr/0022-isometric-board.md); visual-identity S146; ART_DIRECTION S90 | after iso ships + perf/a11y pass |
| **DDR-023** | Chat, shop/currency, levels/XP enter scope as honest inert placeholders (each real later via its own ADR) - REVERSES DDR-013; avatars/counter-offer/replay/houses-hotels-visual are ordinary placeholders; rank stays per-server (DDR-012) (owner 2026-07) | [ddr/0023](ddr/0023-reverse-lean-nongoals.md); DESIGN_PHILOSOPHY | when any of the three becomes real |

## Stability tiers (which decisions are load-bearing identity)

- **Identity - change only with extraordinary reason** (a reversal DDR
  that argues the brand should change): DDR-001, 004, 005, 007, 008,
  009. (DDR-013 was an identity non-goal; the owner reversed it via
  DDR-0023 - the "extraordinary reason" being a deliberate shift toward
  the mockup's richer progression/social framing.)
- **Firm - change with a normal DDR and playtest evidence**: DDR-002,
  003, 006, 010, 011, 012, 014.
- **Expected to evolve**: DDR-015 (relaxed preset), the CVD audit
  outcome, tier ladders, everything in COMMERCIAL_UX_AUDIT.
