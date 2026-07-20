# Motion guidelines (digest)

**Canonical source: `docs/motion-language.md`** - doctrine, tiers,
grammar, budget, the full event catalogue, profiles, architecture. It
is binding via ADR-0030 and enforced by `test/director_test.dart`.
This digest exists for partial-context readers and for review; if it
ever disagrees with the canon, the canon wins.

## The ten laws

1. **Motion is syntax.** An animation exists only to answer: what
   happened / why / to whom / with what consequence / where to look.
   Answering none = deleted.
2. **Readability > beauty; tempo > completeness; truth > both.** Never
   animate a guess; never outspend the 12-second turn.
3. **The board is the protagonist.** Every beat originates or
   terminates on a board object; the HUD is the receipt. The camera
   NEVER moves.
4. **Attention has three devices only**: hairline frame ("subject"),
   lift ("act here"), recede ("nothing else matters" - ~4 uses per
   match; more = misuse).
5. **Tiers are contracts about who waits**: P1 arrest (everyone,
   900-2500 ms, skippable after 400), P2 decide (establish <=900 ms
   then a STATE, not an animation), P3 consequence (300-1000 ms,
   concurrent, never blocks), P4 ambient (<=200 ms implicit
   transitions). Tier is PER-OBSERVER: being attacked is one tier
   louder for the victim.
6. **Money travels** - a chit that STATES itself at the source
   (500 ms), then travels (500 ms). Money is never a number that
   changes; totals update after the receipt arrives.
7. **Shape = category**: chit money, band ownership, chevron VP, rule
   time/structure. **Gold in motion = VP, exclusively.**
8. **Easing = weight; no bounce anywhere, ever.** The one asymmetric
   curve (easeInCubic) is reserved for threats - aggression arrives
   without warning.
9. **The budget is compiled, not hoped**: 8/6/4 s per Update by
   loudest tier, against the server's 10 s ack cap (the two are ONE
   contract - ADR-0028/0030). Compression order: coalesce -> demote ->
   compress -> truncate; P1 never compresses.
10. **Profiles are first-class** (Full / Reduced / Instant); no
    information exists only in motion; motion never gates input;
    reconnect = snap to truth + one 900 ms re-orientation, never a
    catch-up replay.

## Psychology cheat-sheet (what each device is FOR)

Lift = agency ("this is yours to act on"). Recede = gravity ("the
table holds its breath"). Hold/stillness = weight (the P1 payload IS
the stillness). The chit's state-then-travel = comprehension before
narrative. The threat snap = violation (it should feel slightly
unfair - that's the mechanic's honesty). Coalescing = scale (one
estate falling at once reads as catastrophe; eight drips read as
paperwork). Deadpan (the unsold strike-through) = the game's dry wit -
use sparingly, never punch down at a player.

## Adding motion (procedure)

New event -> tier + lane + origin/destination + primitive + duration
in motion-language section 8's table, a fiche if non-obvious, a beat
in `director.compile`'s `_beatsFor`, and a budget test. See
docs/extension-guides.md recipe #2. New PRIMITIVE (a fifth shape, a
fourth attention device) = a DDR, and the answer is probably no.
