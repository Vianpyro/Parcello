# Game feel - the complete feedback loops

For each action that matters: the full loop (visual + audio + motion +
timing) and the feeling it must produce. Motion specifics are canonical
in `docs/motion-language.md` section 8; audio identities in
AUDIO_DIRECTION.md. This file is the synthesis - the "when I do X, the
game answers Y, and I feel Z" contract.

Format: **Trigger -> [visual / motion / audio / timing] -> feeling.**

## Playing a movement card (the every-turn action)

Card lifts from your hand, flips, settles at board centre (350 ms,
easeOutCubic); cardPlay earcon (currently a stand-in clip - see
AUDIO); then the pawn hops tile by tile (260 ms/tile, a soft step
sound per hop, a stop sound on arrival) - the count IS the
information. **Feeling: a deliberate, physical commitment** -
Hearthstone's weight on the one action you repeat forever, without its
tax (skippable, budgeted). The two truth exceptions (teleports that
must not fake a path past Go) are non-negotiable.

## Winning an auction / first purchase

Twelve seconds of drain on the lifted tile -> all sealed bids flip AT
ONCE on the seat markers, hold 300 ms (comparison is the payoff) ->
winner's chit states, travels to the tile, the band SWEEPS to your
colour; cash-loss earcon for you at full price - then, if you
discovered it, the rebate chit travels BACK from the bank with a
cash-gain earcon. **Feeling: a verdict, then ownership of a piece of
the world.** The two-motion price (pay full, get rebate) is a design
decision (ADR-0018 amended): the table must SEE the discoverer's edge.

## Receiving rent (the earner's moment)

Payer's pawn emits a chit (states 500 ms: "amount, from there"), chit
travels to YOUR seat marker, lands, cash-gain earcon, THEN your total
ticks. From the payer's seat the same chit is a loss (sign, colour,
direction, loss earcon). Third parties see it at 60% opacity, silent.
**Feeling for the owner: the engine turning** - income must be
FELT, or property is bookkeeping. (This was the build's largest
readability defect before the chit rule; never regress it.)

## Building

Pool counter ticks down as the building icon RISES into the tile and
settles (Dorfromantik's committed-placement settle); a build earcon
when the audio pass lands (until then: silence, not a wrong sound).
Conglomerate conversion caps the tile in gold - a state the whole
table reads. **Feeling: investment made solid** - and a public step of
the doom clock (the shared pool ticking is deliberately visible to
all).

## Springing a boost trap / being seized

Threat curve (easeInCubic, no warning), oxblood flash ON the
responsible tile, pips shatter, and the rent chit GROWS as it passes
over the trap - the growth is the explanation. Seizure: the band
SNAPS to the aggressor's colour; compensation chit travels to you.
One tier louder for the victim. **Feeling: struck - and immediately
literate about why.** Anger with comprehension is retention; confusion
is churn.

## Drawing a card

Deck tile -> centre flip, face-up HOLD (the read), then the card
discards TOWARD its effect (the causal handoff), cardDraw earcon.
Chains (<=4) play as cause->effect->cause; the budget compiler
truncates long chains rather than rushing them. **Feeling: fate, read
aloud calmly.**

## Winning the game

The final cause plays first at full weight (the last chevron landing /
the pool hitting zero / the clock's hairline emptying) -> recede ->
full-width Deco rule -> winner's seat rises -> STILLNESS (the hold is
the payload) -> arrest earcon: one low, long tone, nothing layered.
Then the scoreboard. **Feeling: arrival, witnessed** - composure at
the top; the win is sold by silence, not fireworks.

## Losing (bankruptcy)

Recede; your pawn greys and lowers IN PLACE (a monument, not an
ejection); your whole estate's bands sweep to unowned in ONE coalesced
motion (1600 ms regardless of size - magnitude is how much board goes
blank, not how long it takes); arrest earcon. Then: you remain at the
table with full view and the survey asks your opinion. **Feeling:
mourned with dignity, still part of the evening.**

## Ranking up / down (ranked end)

ratings_updated arrives with the end staging: your delta shown as a
signed, coloured number NEXT TO your rating (+138 sage / -73 oxblood),
one quiet gain/loss earcon scaled to context (never louder than the
game-end beat it follows). No tier fanfares (no tiers yet; DDR
required). **Feeling: a fair ledger entry** - the ladder respects you
by being matter-of-fact.

## Achievements (future)

If ever added (per-server only until signed results - guide #12):
NEVER interrupt play; a P4 pip at game end, details in the post-game
surface. **Feeling: a nod from the club, not a slot machine.** Any
mid-game achievement toast proposal should be rejected on tempo
grounds alone.

## The meta-loop guarantee

Every loop above ends with the world in a readable static state (pause
any frame: still playable - the profiles guarantee). Feel is the
seasoning; the meal is legibility. When tuning, always cut spectacle
before you cut information.
