# Accessibility

The non-negotiables. Several are already invariants elsewhere (motion
profiles in motion-language.md section 7; layout floor in INVARIANTS
C5; input parity in UX_GUIDELINES). This file states the accessibility
GUARANTEES as commitments that a review can check, and names the gaps
honestly.

## The prime guarantee (already an invariant)

**No information exists only in motion, and nothing important is
conveyed by colour alone.** Every fact an animation carries is also in a
static frame: the band shows the owner, the panel shows the cash, the
log holds the sentence, the tile shows its state. Pause on any frame ->
still playable. This is the single most important accessibility
property and it is enforced by motion-language section 7 + the render
tests. Never regress it: a "quick" motion-only cue is an accessibility
failure.

## Reduced / instant motion

Three profiles, one knob, honoured everywhere (motion-language 7):
Full (1.0), Reduced (0.5, chits fade instead of travel), Instant (0.0,
everything applies immediately - the same first-class path the CLI/bot/
reconnect take, ADR-0028). Default follows the platform "reduce motion"
flag. **Guarantee**: the game is fully playable and fully legible in
Instant. No feature may assume motion happened.

## Colour vision (~8% of players)

Valence is a THREE-channel signal: direction (rises/falls, source->
target), sign (+/-), and colour (sage/oxblood) - colour is the third,
redundant channel, never the first. Group colours are for recognition
(paired with the property NAME, never colour-only). **Known gap**: the
8 group colours + gold + 6 pawn colours have NOT been audited for
specific CVD types (deutan/protan/tritan) for mutual distinguishability;
a colour-blind alternate set is a recorded audit item
(COMMERCIAL_UX_AUDIT, DDR when acted on). Until then: never add a game
meaning that rests on distinguishing two group colours from each other.

## Contrast

`pc-text` on `pc-bg` and `pc-parchment-ink` on `pc-parchment` both meet
WCAG AA for body text; gold on dark meets AA for large/UI text.
`pc-text-faint` is decorative BY DEFINITION - it may never be the sole
carrier of needed information (captions, whispers only). A high-contrast
profile (thicker hairlines, brighter text, stronger focus rings) is a
future addition (audit item) - the flat palette makes it tractable.

## Keyboard & controller

Full focus navigation; every focusable shows a visible gold ring;
Escape/B backs out or skips; the primary action is always reachable and
activatable without a pointer; no hover-only path to anything; no
autofocus on frequently-rebuilt panels (steals focus). Steam Input maps
a gamepad onto this keyboard-focus model - so "keyboard-accessible" and
"controller-accessible" are the same guarantee here.

## Screen readers

**Honest status: not yet addressed.** Flutter Semantics is not wired
for the board or the log. This is the largest accessibility gap and a
COMMERCIAL_UX_AUDIT item. The design that makes it TRACTABLE already
exists: the event log is a complete textual narration of the game
(every event is a localized sentence), and every state has a static
textual form - a screen-reader pass is "expose the existing text via
Semantics", not "invent an accessible mode." Priority: high before a
public commercial launch; the log is the seed.

## Text scaling

Panels must survive platform text scaling to ~1.3x without breakage;
the board's paper labels may cap scaling (the log is the scalable
accessible copy of board events). No text truncation with "..." on
names - wrap or shrink.

## Hit sizes

Minimum 40 px interactive height at panel scale; tile action targets
are the tiles themselves (large). Touch and controller drove this;
mouse inherits it.

## Timing constraints (a real accessibility tension)

The game is deliberately fast (12 s turns, timed windows) - which is an
accessibility CONCERN for players who need more time. Existing
mitigations: the per-turn timer is host-configurable and can be turned
OFF entirely (a present-but-slow player is then never forced); the time
bank gives a personal reserve; AFK auto-play never spends your cash.
**Design stance**: speed is core to the identity, so the answer is
CONFIGURABILITY (untimed rooms, longer turns) rather than slowing the
default. A future "relaxed" preset (longer defaults) is a reasonable
accessibility feature and a clean DDR. Never remove the ability to run
untimed.

## The accessibility review question

For any UI change, ask: does it work in Instant motion, with a
colour-vision deficiency, from a controller, at 1024x600, and (once
wired) under a screen reader? If any answer is no, it is not done.
