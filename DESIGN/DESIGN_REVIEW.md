# Design review

How to review any UI/UX change - the visual twin of AI_ENGINEERING's
review methodology. Run this before shipping a visual change; it is the
distilled form of the rules in this bible.

## The checklist (in order - earlier gates catch cheaper)

### 1. Architecture conformance (blocks merge)

- Does it display any information the server MASKS? (sealed bids, other
  seats' pending votes, private trades - INVARIANTS E4/E5.) If yes,
  STOP: it is unimplementable, not a polish note. This is the
  CONCEPT_CRITIQUE lesson - the prettiest error is the impossible one.
- Does it depict a mechanic the game doesn't have, or contradict an ADR
  (turn/60 counters, global rank, dice)? Route to game design, not
  style.
- Does it introduce a recorded non-goal (chat, shop, XP, energy)? That
  is a reversal DDR + probably an ADR, not a UI change.

### 2. Identity consistency

- Radius 0-2 px? Hairlines not thick bars? No gradient/glow/blur/
  texture? No bounce/spring? (DDR-001, 005.)
- Material correct: dark=machine, parchment=play object, gold=value?
  (ART_DIRECTION contrast law.)
- Colours from the palette only, used per their SEMANTICS? Gold scarce?
  Gold-in-motion = VP only? No third valence invented? (COLOR_SYSTEM.)
- Type voice correct (Fraunces ceremonial / Inter functional / Source
  Serif 4 paper), tabular figures on live numbers? (TYPOGRAPHY.)
- Icons from one set, outlined, quiet, labelled at panel scale? (ICONO.)

### 3. Motion

- Every animation answers at least one of the five questions
  (what/why/who/consequence/where)? (motion-language 1.)
- Correct tier and lane; per-observer where it costs someone; within
  the compiled budget; no camera move; recede not overused? (motion-
  language 3-6.)
- New event -> beat in `_beatsFor` + a director budget test? New
  primitive -> a DDR (and probably no)?

### 4. Readability & cognitive load

- One primary action for the screen state; staging points at it?
- Spatial constancy: nothing recurring moved; no reflow on state
  change?
- Density framed and layered, not dumped? Names not truncated?

### 5. Feedback & error states

- Action feedback <100 ms; consequence on the board; total updates
  after its cause?
- Rejection on the SUBJECT with a reason, never a modal/silent no?
- Things the player was away for get persistent markers, not toasts?

### 6. Responsiveness

- Holds at 1024x600 with LOCALIZED strings (the longest of EN/FR)?
- Persistent UI in a scrolling panel, not floating over the board?
  (INVARIANTS C5 - `layout_test.dart` must still pass.)
- No horizontal page scroll; wide content scrolls in its own box.

### 7. Accessibility

- Playable and legible in Instant motion? Nothing motion-only,
  nothing colour-only?
- Keyboard + controller reachable; visible focus ring; no hover-only
  path; 40 px min hit targets?
- Text scales to 1.3x in panels? New text in BOTH ARB files?

### 8. Design debt & regression

- Any hard-coded hex/duration at a use site? (Bug - belongs in
  tokens.dart / motion.dart.)
- Did it duplicate an existing component instead of reusing it? Add a
  variant, don't fork.
- Did it quietly change a bible rule? Then it needs a DDR + the bible
  update in the SAME change.
- Screenshot the before/after at 1280x800 AND 1024x600; a pumped
  overflow or a shifted seat marker (chits would miss) is a regression.

## Severity language (match the code-review convention)

BLOCKER (architecture/invariant violation, unimplementable, breaks the
layout floor) / MAJOR (identity drift, motion-budget or masking risk,
lost input parity) / MINOR (spacing, a non-semantic colour, a missing
hover earcon) / NIT (wording, optical alignment). Report location +
severity + the bible rule cited, exactly as code review cites an
invariant.

## The one-line test

If a stranger opened this screen with no other Parcello screen visible,
would they know it was the same product, know what to do next, and be
unable to see anything the game is hiding? If any answer is no, it's
not done.
