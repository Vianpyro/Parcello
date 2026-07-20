# Audio direction

The sound identity. Ground truth for the current wiring:
`clients/flutter/assets/sfx/README.md` (file names, drop-in slots,
missing-file-is-silent) and `lib/sfx.dart`. The clip set is a
PLACEHOLDER today; the four category earcons are silent. This document
is the brief the audio pass must satisfy.

## Philosophy

**Sound is a property of a beat's CATEGORY, never of the event**
(motion-language section 4): one identity per category, reused
everywhere, so players absorb the vocabulary unconsciously - the audio
twin of "gold-in-motion means VP". Corollaries: few sounds, absolute
consistency, and SILENCE as a first-class instrument.

Register: brass-and-paper chamber, not arcade. Think felt-dampened
piano lid, a coin on wood, a stamp on paper, one low clarinet/cello
tone - acoustic-adjacent, dry, short decays. Forbidden: synth risers,
sparkle glissandi, crowd cheers, slot-machine payout loops, any sound
with a "casino" or "mobile juice" register.

## The four earcons (the vocabulary - highest priority work)

| Earcon | Meaning | Direction |
|---|---|---|
| `cash-gain` | money arrived AT YOU | warm, rising 2-note figure, <300 ms |
| `cash-loss` | money left YOU | the same figure inverted/falling - a mirrored pair, learnable as one shape |
| `card-draw` | fate revealed | a single paper flick + soft mallet tone |
| `arrest` | P1 - the table stops | ONE low, long tone (>=1.2 s), never layered, shared by bankruptcy and victory - the SAME bell tolls for both, and that is the register |

Third-party money is SILENT by rule (six seats of transfers is a wall
of noise); you hear only your own economy. Threat moments (trap,
seizure) ride `error`'s family today; the audio pass may give threat
its own short dry impact - a fifth category at most, DDR'd.

## UI chrome (already wired, keep quiet)

Hover ticks, confirm yes/no, join/leave, toggle, game-start, timer
milestones (60/30/10/5..0 s): all sub-200 ms, low level, dry. The
timer pips are the only permitted urgency sound and must stay gentle -
urgency lives in the draining hairline; audio only seconds it.

## Music

**In-game: none.** A 12-second decision rhythm plus voice chat (the
expected social context) leaves no room for score; ambience would fight
the earcon vocabulary's clarity. Menu/lobby: at most ONE sparse,
loopable Deco-flavoured cue at low level, default ON in menus, ducked
to silence when a game starts; end-of-game may land a single short
musical resolution (2-4 s) under the arrest tone's tail. All of this
is a proposal for the audio pass - record the final call as a DDR.
Silence is the default answer to "should this have sound?".

## Mixing rules

Never two earcons in the same 150 ms window (the director coalesces;
audio follows the coalesced beat, one sound for eight tiles). P1
ducks everything. Master volume + a mute all in settings; respect
platform mute. No audio-only information, ever (deaf players lose
seasoning, never facts) - the inverse of the motion guarantee.

## Known debts

The `dice-roll.mp3` stand-in on card play (no dice since ADR-0017 -
the file's own README flags it); the four earcons missing; haptics on
Deck (P1 + threat) specified in motion-language 14 and unbuilt.
