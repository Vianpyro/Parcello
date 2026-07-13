# Parcello motion language

Status: reference document for the Flutter client (`clients/flutter`).
Companion to `docs/visual-identity.md` (what the game looks like); this one
is what the game *does* when state changes, and why. Binding on client
code. No engine impact anywhere in this document.

Precedence: `docs/architecture.typ` > ADRs (0026 spotlight, 0028
animation-ack watermark, 0030 animation budget) > this document >
implementation. Where this document constrains client behavior against a
server constant, ADR-0030 records the contract.

---

## 1. Doctrine

**Motion is the game's syntax, not its decoration.**

Parcello is an information race. Turns are 12 seconds; a game is 30
minutes; the win condition is a victory-point sprint that every player is
tracking simultaneously. A player's whole job is to read the board and
decide. So motion has exactly one job: **make a state change cheap to read,
impossible to miss, and then get out of the way.**

An animation exists only if it answers at least one of:

1. What just happened?
2. Why did it happen?
3. Who is affected?
4. What is the consequence?
5. Where should I look now?

If it answers none of these, it is deleted. Not shortened - deleted.

Three corollaries, in tension, and the tie-breaks between them:

- **Readability beats beauty.** When a beautiful motion and a legible one
  disagree, ship the legible one.
- **Rhythm beats completeness.** A 12-second turn cannot afford a
  2-second flourish for a routine event. When an animation and the game's
  tempo disagree, the tempo wins - compress or drop the animation. This is
  not a preference; it is a hard budget (section 5).
- **Truth beats both.** Motion must never imply a state the server did not
  send. An optimistic animation that gets rejected is a lie the player will
  pay for later. Render server truth, never a guess.

---

## 2. The board is the protagonist

Everything that happens in Parcello happens *to a tile* or *to a player*.
Both live on the board. Therefore:

- **Every animation originates at, or terminates on, a board object.** A
  tile, a pawn, a seat marker. Nothing "just appears" in a HUD.
- **The HUD echoes; it never replaces.** A cash total in the side panel
  ticks *because* a chit arrived from a tile. If the player watched the
  board, they already know the number before the HUD confirms it. The HUD is
  the receipt, not the event.
- **The camera never moves.** No zoom, no pan, no shake, no scroll during
  play. Parcello is a game of perfect information about a fixed space, and a
  competitive player's spatial memory of that space is an asset - moving the
  camera destroys it, and forces a re-scan on every event. This is the single
  most valuable thing to take from *Into the Breach*.

"Attention" is therefore never expressed by moving the camera. It is
expressed by exactly three devices, in escalating order:

| Device | Meaning | Used by |
| --- | --- | --- |
| **Hairline frame** - a 1-2 px gold rule drawn around a tile | "this tile is the subject" | P3 |
| **Lift** - the tile raises 2 px with a hard shadow, +2% scale | "act on this tile" | P2 |
| **Recede** - everything *except* the subject drops to 35% opacity | "nothing else matters right now" | P1, and P2 for the sealed bid only |

Recede is the strongest instrument in the game. It is used four times in a
typical match. If it is used more, it is being used wrong.

---

## 3. Information hierarchy

Four tiers. A tier is not a style - it is **a contract about who waits**.

### P1 - ARREST. The table stops.

Irreversible, game-defining. Everyone must see it; nobody may act during it.

- **Budget:** 900-2500 ms, with a **hold** (stillness) as the payload.
- **Blocks:** the beat queue, all input, all timers already gated by
  ADR-0028.
- **Visual:** *recede* the whole board, subject at full opacity. Full-width
  Deco rule sweeps in. Flat colour, no particles.
- **Sound:** one low, long earcon. Never layered.
- **Skippable:** after 400 ms, by any input. The hold is skippable; the
  information beat is not.
- **Members:** `PlayerBankrupt`, `GameEnded`, `TimeUp`, `WonByPoints`,
  `WonByGroups`, `WonByPoolExhaustion`.

### P2 - DECIDE. The player must act, and the clock is running.

A window is open. The player's next 5-12 seconds are spent here.

- **Budget:** 400-900 ms to *establish*, then **the animation is over** and a
  persistent mode remains.
- **Blocks:** the beat queue until the subject is visually established -
  this is precisely the ADR-0028 guarantee ("nobody bids on a property they
  have not seen").
- **Visual:** *lift* the subject tile; anchor the input to it; run the
  window clock as a **draining hairline on the tile's own edge**, not as a
  number in a corner.
- **Skippable:** the establish beat, yes. The mode, obviously not.
- **Members:** `BlindAuctionOpened`, `BribeOffered` (opens the vote),
  `TradeProposed` (addressed to you), `TurnStarted` (you), plus the
  non-engine moment "your time bank has started draining".

> **A decision is not an animation. It is a state.** The animation only
> transports the player into it. Everything after the establish beat is
> steady-state UI, and steady-state UI does not move. This is the rule that
> keeps a competitive game from feeling like a slot machine.

### P3 - CONSEQUENCE. Money, property, or points moved.

The substance of the game. Frequent, must be readable at a glance and in
peripheral vision, must never block.

- **Budget:** 300-700 ms, overlappable.
- **Blocks:** nothing structurally. Beats in this tier may run concurrently
  with each other (section 6).
- **Visual:** the travelling primitives of section 4. Always source -> target.
- **Sound:** one short earcon, category-typed (gain / loss / build / seize).
- **Members:** most events. See the catalogue (section 8).

### P4 - AMBIENT. Context. No decision, no consequence.

- **Budget:** 0-200 ms. Never enters the beat queue at all.
- **Blocks:** nothing, ever.
- **Visual:** an implicit transition on the widget that owns the state
  (`AnimatedOpacity`, `AnimatedContainer`). The state changes; the
  transition is how it changes; there is no "animation" as an object.
- **Sound:** none, with one exception (`BlindBidSubmitted` gets a soft tick
  - it is the only signal that the auction is progressing).
- **Members:** `BlindBidSubmitted`, `BribeVoteCast`, `TradeDeclined`,
  `TradeCancelled`, `MarketEventExpired`, `SpotlightEnded`,
  `JailCardReceived`, connection state, lobby churn.

**The tier table:**

| | Who waits | Budget | Intensity | Skippable | Concurrency |
| --- | --- | --- | --- | --- | --- |
| P1 | Everyone | 900-2500 ms | Recede + full-board rule | After 400 ms | Exclusive |
| P2 | The deciders | 400-900 ms | Lift + anchored input | Establish only | Exclusive |
| P3 | Nobody | 300-700 ms | Travelling primitive | Yes | Concurrent (capped) |
| P4 | Nobody | 0-200 ms | Implicit transition | N/A | Free |

---

## 4. The grammar

A small alphabet, composed. The player is never taught it; they absorb it
because it never varies.

### 4.1 Direction encodes valence

| Motion | Meaning |
| --- | --- |
| **Rises** and settles | gain, acquisition, growth |
| **Falls** | loss, payment, decay |
| **Travels A -> B** | a transfer between two named parties |
| **Sweeps** along an edge | a change of ownership or of level |
| **Drains** (a rule shortening) | time running out |

### 4.2 The money rule (the single highest-value rule in this document)

**Money is never a number that changes. Money is an object that travels.**

Every cash movement renders as one parchment **chit** that leaves a source
and lands on a target:

| Event | Source | Target |
| --- | --- | --- |
| Rent | the payer's pawn | the owner's seat marker |
| Salary | the Go tile | the player's seat marker |
| Tax | the payer's pawn | the tax tile |
| Auction settlement | the winner's seat marker | the tile |
| Bribe payout | the briber's pawn | each opponent's seat marker |
| Card money | the card banner | the player's seat marker |

Consequences of adopting this rule, all of them free:

- "Who paid whom" is never a question. It is the shape of the motion.
- Rent stops being invisible **to the person being paid**. Today the client
  floats only the payer's `-$50`; the owner - who just earned the game's
  core income - sees nothing. That is the largest single readability defect
  in the current build.
- A player watching peripherally still reads the *direction* of the economy
  even if they cannot read the number.

The chit carries its amount in Inter tabular figures. Gain-coloured
(`pc-sage`) landing on you, loss-coloured (`pc-oxblood`) leaving you. The
same chit is both, seen from two seats - which is correct, and is exactly
what makes it legible.

### 4.3 Shape encodes category

Art Deco means geometry does the work. Four shapes, four meanings, no
overlap:

| Shape | Category | Colour |
| --- | --- | --- |
| **Chit** (small parchment rectangle, 0 px radius) | money | sage / oxblood |
| **Band** (the tile's group-colour edge stripe) | ownership | the owner's pawn colour |
| **Chevron** (a stepped Deco arrow) | victory points | `pc-gold`, and *only* VP is gold-in-motion |
| **Rule** (a 1-2 px hairline) | time, framing, structure | `pc-gold` / `pc-border` |

Gold that *moves* always means victory points. Nothing else in the game is
allowed to move in gold. That is how a player learns, without being told,
that the gold chevron flying to a rival's counter is the thing that loses
them the game.

### 4.4 Easing encodes weight - and there is no bounce

| Tier | Curve | Feel |
| --- | --- | --- |
| P4 | `linear` / `easeOut`, 120 ms | unnoticed |
| P3 | `easeOutCubic`, 300-500 ms | decisive arrival |
| P2 | `easeInOutCubic`, 600 ms | deliberate |
| P1 | `easeOutQuint` + hold | inevitable |
| Threat (takeover, boost trap, bankruptcy) | **`easeInCubic`** - snap in, no ease-in ramp | something was done *to* you |

**No springs. No elastic. No bounce. No squash-and-stretch. Anywhere.**

This is an identity decision, not a taste one. Bounce reads as toy, casual,
mobile-free-to-play. Art Deco is arrival and symmetry: motion resolves and
*stops*. Restraint in the easing curve is what makes the palette's restraint
believable. One inconsistent bouncy element would undo the whole register.

The only asymmetric curve in the game is the threat curve, and its
asymmetry *is* the message: aggression arrives without warning and lingers.

### 4.5 Colour in motion

Motion may only use colours that already exist in `visual-identity.md`.
Motion introduces no new hues.

| Colour | In motion means |
| --- | --- |
| `pc-sage` `#3F5240` | you gained |
| `pc-oxblood` `#9C433A` | you lost / you are threatened |
| `pc-gold` `#D8B45A` | victory points; framing; time |
| pawn colour | ownership, identity |
| `pc-parchment` | paper: chits, cards, receipts |

---

## 5. The animation budget (hard constraint)

ADR-0028 gates server timers on client render acks, bounded by
`ANIM_ACK_CAP` (`crates/server/src/room.rs`, now 10 s). Past the cap the
server proceeds **without** the client. A client whose beats outrun the cap
is therefore not merely slow - it is *behind the game*: the bid window opens,
or a bot moves, while it is still animating the previous turn. That is the
exact desynchronisation ADR-0028 was written to prevent.

**The build before this document violated it.** A single `Update` could
chain: movement (1970 ms) + card reveal (1700 ms) + card-driven teleport
through Go (2810 ms) + salary (500 ms) = **~6980 ms**, over the then-6 s cap.
Card chains recurse to `MAX_CARD_CHAIN_DEPTH = 4`, so the worst case was far
past it. Nothing bounded the *sum* of the beats, which is the whole reason
the budget below is a compile-time property and not a hope.

**The rule:**

> **No `Update` may exceed the budget set by the loudest beat in it** - 8 s
> when it carries a P1, 6 s for a P2, 4 s otherwise - against a server
> `ANIM_ACK_CAP` of 10 s (a 2 s margin for frame-rate slop and a slow first
> paint).

The budget is tiered along the tiers because the tiers already say *who is
waiting and why*. A bankruptcy or a win is the moment the whole table stops
for, and it can afford eight seconds. A routine move cannot - it happens every
twelve. (A flat 4 s was the first cut; the first full playtest showed it
rushed the moments that matter and bought nothing on the ones that don't.)

The scheduler enforces this by **compiling the whole Update before playing
any of it** - the plan's cost is known up front - and compressing when over
budget, in this order:

1. **Coalesce** same-kind beats on the same subject (section 6).
2. **Demote** P3 beats to their instant form (state applies, no travel).
3. **Compress** the exclusive lane: scale P3 durations down, floor 40%.
4. **Truncate**: play the first and last beat of the chain, apply the rest
   instantly. The first tells you where it started, the last where it ended.

P1 beats are never compressed. If a bankruptcy and a 4-deep card chain land
in the same Update, the card chain is what gets cut. That is the correct
priority and it falls out of the tiers automatically.

Recording the budget as a client invariant against a server constant is
what ADR-0030 exists for.

---

## 6. Simultaneity

An `Update` is a burst of events. They are not a queue of animations - they
are a **plan**, compiled.

### Lanes

- **Exclusive lane** - one at a time, the plan waits. Pawn movement, card
  reveal, jail hop, every P1 and P2 beat.
- **Concurrent lane** - up to 6 in flight, the plan does *not* wait. Chits,
  chevrons, band sweeps, tile-state changes.

A concurrent beat that shares a *subject* with the next exclusive beat is
promoted to exclusive (you must not slide a pawn off a tile while a chit is
still landing on it).

### Coalescing

**N same-kind events on the same subject collapse into one beat of scaled
magnitude.** This is not an optimisation - it is a readability rule.

The motivating case: a bankruptcy transfers a whole portfolio, emitting one
`PropertyTransferred` per tile. Eight sequential 400 ms band sweeps is 3.2 s
of drip. One beat in which **all eight bands sweep to the creditor's colour
at once, staggered 40 ms**, is 720 ms, and it reads as what it actually was:
an estate changing hands in one motion. Same for forced liquidation's
`HouseSold` burst, and for the bribe payout's N `CashAdjusted` events (one
chit per opponent, fired together).

### Ordering

Beats play in server-emitted order. The engine already orders events
causally (`Moved` before `CardDrawn` before the effect), and that ordering
is the animation script. The client never reorders - it only groups.

---

## 7. Accessibility and motion profiles

Three profiles. One knob, honoured everywhere, no exceptions.

| Profile | Duration scale | Behaviour |
| --- | --- | --- |
| **Full** | 1.0 | as specified |
| **Reduced** | 0.5, no travel | chits *fade* at the target instead of travelling; state changes cross-fade; the plan still paces |
| **Instant** | 0.0 | every beat applies immediately; the ack fires at once |

**Instant is not a degraded mode - it is a first-class path**, and it is the
same one the CLI and bot seats already take (ADR-0028: "the same path a
future reduced-motion setting takes"). This is why the profile knob is safe:
the server has always had to tolerate a client that never animates.

Guarantees that hold in every profile:

- **No information exists only in motion.** Every fact an animation conveys
  is also readable from a static frame: the tile's band shows the owner, the
  side panel shows the cash, the log holds the sentence, the tile shows
  `SPOTLIGHT`. Motion makes a fact *cheap*; it is never the *only* copy of
  it. Test: pause on any frame and the game must still be playable.
- **Nothing important is conveyed by colour alone** (the ~8% of players with
  a red-green deficiency must read gain/loss): a chit's *direction* and its
  `+`/`-` sign carry the valence; colour is the third, redundant channel.
- **Motion never gates input.** A player may act during any beat except P1's
  400 ms information window. Skipping is always available (any key, any
  click).
- Respects the platform's "reduce motion" accessibility flag as the default
  value of the knob.

---

## 8. Event catalogue

All 43 engine events (`crates/engine/src/event.rs`), plus the non-engine
moments the naive list forgets (8.4).

The per-event fiche the brief asks for has 15 fields; 43 x 15 is a matrix
nobody reads and nobody maintains. Most of those fields are **policies, not
per-event facts** - skip behaviour, reconnect behaviour, and simultaneity
are defined once in sections 3, 6, 7 and 9 and apply uniformly. What is
genuinely per-event is: tier, lane, subject, origin, destination, primitive,
duration. That is the table. The events whose *design* is non-obvious get a
prose fiche after it.

### 8.1 The table

Legend: **lane** X = exclusive, C = concurrent, - = not a beat (P4 implicit
transition). "Seat" = the player's marker in the side panel.

| Event | Tier | Lane | Origin -> Destination | Primitive | ms |
| --- | --- | --- | --- | --- | --- |
| `TurnStarted` | P2 (you) / P4 | X / - | - -> your seat | seat marker lights; board rule sweeps once | 300 |
| `MovementCardPlayed` | P3 | X | hand -> board centre | card lifts out of the hand, flips, settles | 350 |
| `Moved` | P3 | X | tile -> tile | pawn hops tile by tile | 260/tile |
| `SalaryPaid` | P3 | C | Go tile -> seat | chit (gain) | 450 |
| `BlindAuctionOpened` | **P2** | X | tile | **recede + lift**; bid input anchors to tile; edge hairline drains | 700 |
| `BlindBidSubmitted` | P4 | - | that seat | seat marker gets a sealed dot | 120 |
| `BlindAuctionResolved` | **P3** | X | all bids -> tile | **bids flip face-up on each seat**, winner's chit travels to tile, band takes their colour | 1100 |
| `TradeProposed` | P2 (to you) / P4 | - | proposer seat -> your seat | offer card slides into the trade panel; seat marker pulses | 300 |
| `TradeAccepted` | P3 | C | seat <-> seat | chits + bands cross in both directions simultaneously | 600 |
| `TradeDeclined` | P4 | - | - | offer card dissolves | 150 |
| `TradeCancelled` | P4 | - | - | offer card dissolves | 150 |
| `RentPaid` | **P3** | C | payer pawn -> owner seat | **chit travels** (loss at source, gain at target) | 500 |
| `TaxPaid` | P3 | C | payer pawn -> tax tile | chit (loss); tile flashes oxblood | 500 |
| `CardDrawn` | P3 | X | deck tile -> centre | card flips face-up, held, then discards toward its effect | 1200 |
| `CashAdjusted` | P3 | C | card banner -> seat | chit | 450 |
| `HouseBuilt` | P3 | C | pool counter -> tile | building icon **rises** into place; pool counter ticks down | 400 |
| `HouseSold` | P3 | C | tile -> pool counter | building **falls** out; chit to seat; pool ticks up | 400 |
| `Expropriated` | **P3** (threat) | X | seizer pawn -> tile | band **snaps** to seizer's colour (`easeInCubic`); compensation chit to former owner | 700 |
| `RentBoosted` | P3 | C | seat -> tile | tile gains a boost pip; chit (loss) to bank | 400 |
| `RentBoostConsumed` | **P3** (threat) | C | tile | **the trap springs**: pips shatter outward, tile flashes oxblood once | 350 |
| `PropertyMortgaged` | P3 | C | tile -> seat | band **desaturates to hatching**; chit (gain) | 400 |
| `PropertyUnmortgaged` | P3 | C | seat -> tile | hatching clears; chit (loss) | 400 |
| `WentToJail` | P3 | X | `from` -> jail tile | pawn slides straight (never hops); jail bars wipe across the tile | 800 |
| `JailCardReceived` | P4 | - | seat | a card pip appears on the seat marker | 150 |
| `JailCardUsed` | P3 | C | seat -> jail tile | card pip flies to the tile and dissolves the bars | 450 |
| `LeftJail` | P3 | C | jail tile | bars wipe away | 300 |
| `LegalRouteChosen` | P3 | X | seat | the route's cards lay out face-up in order under the seat; owner's tiles dim (rent-free) | 600 |
| `BribeOffered` | **P2** | X | briber pawn -> table | **recede**; the offer sits on the briber's tile; vote buttons anchor there; hairline drains | 700 |
| `BribeVoteCast` | P4 | - | that seat | sealed dot on the seat marker | 120 |
| `BribeResolved` | **P3** | X | briber -> each opponent | votes flip face-up; on success N chits fan out to every opponent at once (coalesced), bars dissolve | 900 |
| `PropertyTransferred` | P3 | C | tile | band sweeps to the new owner's colour (**coalesced** across a portfolio) | 400 + 40/tile |
| `PlayerBankrupt` | **P1** | X | that player | **recede**; pawn greys and lowers; the whole portfolio's bands sweep to the creditor in one motion | 1600 |
| `PlayerResigned` | P3 | X | that player | pawn greys and lowers (no recede - resigning is not an event *to* the table) | 700 |
| `GameEnded` | **P1** | X | winner | recede; Deco rule sweeps full width; winner's seat rises | 2000 |
| `TimeUp` | **P1** | X | clock -> winner | the game clock's hairline empties, then the win beat | 2200 |
| `WonByGroups` | **P1** | X | winner's groups | the winning groups' bands ignite in sequence, then the win beat | 2200 |
| `WonByPoints` | **P1** | X | VP counter | the final chevrons land, counter hits target, then the win beat | 2200 |
| `WonByPoolExhaustion` | **P1** | X | pool counter | **the pool counter hits zero and the board goes still**, then the win beat | 2400 |
| `MarketEventActivated` | **P2**/P3 | X | forecast strip -> board | the forecast slot **slides left into the active slot**; board edge takes the effect's tint | 800 |
| `MarketEventExpired` | P4 | - | active slot | the tint drains away | 200 |
| `RoundBonusAwarded` | **P3** | C | round pips -> leader's seat | pips complete, **gold chevron** flies to the leader's VP counter | 600 |
| `SpotlightStarted` | P3 | X | Exposition -> tile | a gold rule travels from the Exposition corner to the tile; tile frames in gold | 900 |
| `SpotlightEnded` | P4 | - | tile | gold frame fades | 200 |

### 8.2 Fiches - the events whose design is not obvious

**`BlindAuctionOpened` / `BlindAuctionResolved` (ADR-0018) - the game's core loop.**

Today: a text line and buttons in a panel. This is the decision the whole
game is built on, and it has no moment.

- *Opened* (P2): the board **recedes**; the discovered tile **lifts**. The
  bid field appears anchored to the tile - you are bidding *on that thing*,
  not filling in a form. The 12 s window is a gold hairline **draining along
  the tile's own edge**. The discoverer's floor bid shows as a ghost value
  already in the field. No number-in-a-corner countdown: the tile is the
  clock.
- *Resolved* (P3): the payoff. Every seat's sealed bid **flips face-up on
  their marker simultaneously** - this is the single most information-dense
  moment in Parcello and it is currently invisible. Beat: flip (300 ms),
  hold (300 ms) so the table can compare, then the winner's chit travels to
  the tile and the band takes their colour (500 ms). The 90 % contested-win
  discount, if it applied, shows as the chit *shrinking* mid-flight - the
  discount is a thing that happens to the money on its way.
- Unsold (all-zero): the tile drops back with no band, and a single
  `pc-text-faint` rule strikes through it. Deadpan. That is the joke.

**`PlayerBankrupt` (P1) - the only compound P1.**

Recede. The pawn greys and lowers *in place* (it does not leave the board -
it is a monument). Then the portfolio: every transferred tile's band sweeps
to the creditor **in one coalesced motion**, staggered 40 ms, so a large
estate reads as a single event and not a drip. Then the hold. 1600 ms total
regardless of portfolio size - an 18-tile bankruptcy and a 2-tile one take
the same time, because the *information* is the same ("X is out, Y took
everything"), only the magnitude differs, and magnitude is conveyed by how
much of the board changes colour at once. That is the coalescing rule paying
for itself.

**`RentBoostConsumed` (ADR-0012, one-shot trap) - the pure "why did that happen?" case.**

A boost is a trap the owner armed turns ago. It springs once, and today it
springs silently: the victim sees a large rent number and no reason for it.
The beat must land *between* the rent chit leaving and arriving, and it must
be legible **to the victim**: the tile's boost pips shatter outward, the tile
flashes oxblood once, and the rent chit *grows* as it passes over the tile.
The chit growing is the causal link - the trap is why the number is that big.
Cost: 350 ms, and it converts the game's most confusing moment into its most
satisfying one.

**`MarketEventActivated` (ADR-0021) - the forecast is a promise; activation is it being kept.**

The tile is where the promise is kept, not just the strip. While an
`acquisition_multiplier` is active (the base mod's Market Bubble, -30%), the
list price printed on a property **is not the price** - it is not what a
sealed-bid winner settles at, nor what a takeover costs. So the tile shows the
effective number with the old one beside it (`$72 (was $104)`), tinted by the
grammar the player already knows: cheaper to take reads as a gain, dearer as a
loss. A `rent_multiplier` (Market Crash) does *not* touch prices, and the tile
must not pretend it does - the grammar only works while it never lies.

The forecast strip is a queue the player has been watching for three turns.
When an event fires, it must be *the same object* arriving - the slot
**slides left into the active position**. Never a popup, never a fresh
banner: a popup would break the promise the strip made. The board's outer
frame then takes the effect's tint (oxblood for a crash/tax, sage for a
bubble) for the duration, so the modifier is a persistent, ambient property
of the world rather than a fact you must remember. P2 for a `wealth_tax`
(everyone pays now - a real decision follows), P3 otherwise.

**`RoundBonusAwarded` (ADR-0020) - the one non-reversible VP source.**

The round metronome is currently a row of pips and a sentence. When the last
straggler cycles their hand, the pips complete, and a **gold chevron** flies
from the pip row to the cash leader's VP counter. Gold-in-motion means VP,
always. The chevron is how a player learns, three rounds in and without ever
reading the rules panel, that being richest at the round boundary is worth
something permanent - and starts fighting for it.

**`Moved` - the hop, and the two lies it must not tell.**

The pawn hops tile by tile because *the count is the information* (a 5-card
moved you 5 tiles - you should be able to count it). Two exceptions, both
already discovered by playtest and both preserved:

- A card teleport that does **not** pass Go must **glide straight**, or it
  appears to cross Go and the player expects a salary that never comes.
- A card teleport that **does** pass Go (`passed_go`) must **hop the whole
  way**, or the `+$200` chit has no visible cause.

Both are the truth rule (section 1) in miniature: the motion may not imply a
path the engine did not take. Under the budget rule, a long forced hop
tapers its per-tile rate rather than dropping the hop.

### 8.3 What plays for whom

The same event is not the same event from every seat. Tier is
**per-observer**:

| Event | For the actor | For the target | For everyone else |
| --- | --- | --- | --- |
| `TurnStarted` | P2 (act now) | - | P4 |
| `TradeProposed` | P4 (sent) | **P2** (decide) | not delivered (ADR-0007) |
| `RentPaid` | P3 loss | P3 gain | P3, quieter (60 % opacity) |
| `Expropriated` | P3 | **P2-intensity** (attacked) | P3 |
| `BlindAuctionOpened` | P2 (+ floor ghost) | P2 | P2 |

Rule: **an event that costs you something is always at least one tier
louder for you than for the table.** Being attacked is never ambient.

### 8.4 The events the list forgets

These have no engine `Event`, and every one of them is a real hole today:

| Moment | Why it matters | Treatment |
| --- | --- | --- |
| **You were AFK auto-played** | The server plays your canonical action (ADR-0017/0024). You return, your card is gone, and *nothing ever told you*. | **P2.** A persistent, dismissible marker on your seat: "auto-played: card 4". Not a toast - a toast you were away for is a toast you never saw. |
| **Hand refill** (`hands_cycled` tick) | Drives the round metronome and therefore the +2 VP - and emits no event at all. | P4: the hand's cards deal back in. The round pip fills. Derived client-side from `hand`/`hands_cycled` in the view. |
| **Command rejected** | Currently a log line and an error sound. *Which* command? *Which* tile? | P3: the **subject** rejects - the tile or button shakes 3 px laterally, once, and the reason prints on it. Never a modal. Errors are the one place a 3 px lateral shake is allowed, because "no" is a physical gesture. |
| **A bot is thinking** (`BOT_THINK` 800 ms) | 800 ms of nothing looks like a hang. | P4: the bot's seat marker shows a quiet working pulse. |
| **Your time bank starts draining** (ADR-0023) | A silent, irreversible resource starts burning. | **P2.** The turn hairline turns oxblood and the bank number begins to move. This is the most expensive thing that can silently happen to you. |
| **Reconnect mid-game** | Director resets and snaps to truth (correct), but the player is disoriented. | See section 9. |
| **Spotlight expiring by turn count** | It just vanishes. | P4: the gold frame drains over the last turn rather than blinking out. |
| **Connection lost / restored** | | P4 banner, non-blocking; the board greys 20 % while disconnected so the player never mistakes a frozen board for a slow turn. |

---

## 9. Reconnection

One rule, applied everywhere: **on reconnect, the client renders the
present, never a replay of the past.**

- The director's plan is aborted (`_updateEpoch` already does this), the
  stage snaps to the authoritative view, and the ack fires immediately.
- **No catch-up animation, ever.** Animating 20 seconds of missed events is
  the single worst thing a reconnecting client can do: it is behind the game
  and it is spending its budget on history rather than on the decision it is
  now late for.
- What the player gets instead is **re-orientation**, once, for 900 ms:
  their own pawn pulses, their seat marker lights, and the turn indicator
  sweeps to whoever is acting. "Here is you, here is now."
- The event log is the record of what was missed. That is the log's job, and
  the reason it survives the redesign.

The same rule covers a player who tabs away and returns: the browser
throttles timers, the beats are stale, the answer is the same - snap, then
re-orient.

---

## 10. Flutter architecture

### 10.1 Why the current one has to change

`session.dart` holds four jobs: WebSocket transport, auth/session identity,
authoritative game state, and animation direction. The director is a
`switch` of `await Future.delayed(...)` with durations inline. Three
concrete consequences, all of them already visible in the code:

1. **Duration knowledge is duplicated.** `GameSession._moveDuration()`
   re-derives what `board.dart`'s `_PawnLayer` does, and its own comment
   admits it ("mirrors `_PawnLayer`'s hop timing ... so a beat waits for the
   glide it just triggered"). Two sources of truth for one number, kept in
   sync by hand.
2. **Every beat rebuilds the whole tree.** `notifyListeners()` on each beat
   repaints the board, the side panel *and the action panel* - including its
   text fields. The code carries two guards (`_bidInitTile`, `_bribeSeeded`)
   that exist solely to stop an animation frame from wiping a half-typed bid.
   Those are not bugs to fix individually; they are the symptom of transient
   visual state and durable input state sharing one notifier.
3. **The animation logic is untestable.** It is `async` control flow inside a
   `ChangeNotifier` that owns a socket. There is no way to assert "a
   bankruptcy of an 18-tile estate stays under budget" without a render tree
   and a server.

### 10.2 The target

Five objects, one job each. The dependency arrow points one way, exactly as
the Rust side does it.

```
GameSession        transport + auth + authoritative view.   Notifies on TRUTH.
  |  Update
  v
AnimationDirector  compile(events, view) -> Plan    <- PURE, unit-testable
                   execute(Plan) against a clock
  |  mutates
  v
StageState         transient visual state ONLY.     Notifies on FRAMES.
  |  read by
  v
BoardWidget / HUD  dumb projections of Stage + View

MotionSpec         const tokens: tiers, durations, curves, budget, profiles
Tokens (theme)     const palette + shapes from visual-identity.md
```

- **`compile()` is a pure function** `(List<Event>, ClientView) -> Plan`. No
  socket, no widgets, no clock. This is the whole point: the budget rule,
  the coalescing rule, the tier assignment and the lane assignment are all
  decided in a function that a unit test can call in a loop. The invariant
  "no plan exceeds its budget" becomes an assertion, not a hope.
- **`StageState` is a separate notifier** from `GameSession`. Animation
  frames repaint the board; they do not touch the action panel. Concretely:
  the action panel is built by `GameScreen` (which only rebuilds on server
  truth) and handed to `BoardWidget` as `center`, so on an animation frame it
  is the *same widget instance* and Flutter reuses its element - text fields
  and all - untouched. (The `_bidInitTile` / `_bribeSeeded` fields stay: they
  also carry real seeding logic - "seed the bid at the list price when a *new*
  auction opens" - so they are no longer load-bearing as guards, but they are
  not dead either.)
- **`MotionSpec` is the only place a duration exists.** `board.dart` reads
  its hop rate from there; the director reads the same constant. The
  duplication in 10.1(1) becomes structurally impossible.
- **The profile knob multiplies at exactly one place** - the executor's
  wait. Reduced and Instant fall out for free; so does the ADR-0028
  auto-ack path.

### 10.3 The beat

```dart
sealed class Beat {
  Tier   get tier;
  Lane   get lane;         // exclusive | concurrent
  Duration get cost;       // what the plan pays for it
  void apply(StageState s);
}
```

`Plan` = `List<Beat>` + a total cost known before the first frame - which is
what makes the budget enforceable *before* anything is shown, rather than
discovered halfway through.

### 10.4 What stays

The ADR-0028 contract is untouched: Updates still queue, still play in
order, the view still applies after the beats, the ack still goes out with
it. `_updateEpoch` still aborts a stale plan. This is a restructuring of the
director, not a renegotiation of the protocol.

---

## 11. What the reference games actually teach

Analysed for readability, game feel, hierarchy and UX. None of them is
copied stylistically - Parcello's register (Art Deco, flat, restrained) is
the opposite of most of them.

**Into the Breach - the most important one.**
Perfect information, zero camera movement, and *every threat is drawn on the
board itself* as an arrow on the tile it will hit. The player never holds
state in their head. Parcello takes: the fixed camera (section 2), threats
rendered *on their target* (`RentBoostConsumed` on the tile, not in a
banner), and the doctrine that a competitive game's UI must be a map, not a
feed.

**Hearthstone - the moment, and its cost.**
Hearthstone makes a card play feel *physical* (weight, arrival, impact) and
that is why the game feels good after 1000 hours. It also demonstrates the
failure mode: a slow, unskippable animation on a frequent action becomes a
tax you pay forever. Parcello takes the physicality of the card play
(`MovementCardPlayed` lifts, flips, settles) and rejects the tax:
everything is skippable, and the budget is enforced in the compiler.

**Legends of Runeterra - the best-in-class answer to "who is doing what".**
LoR's alternating priority is legible because the game *shows the pending
action on the board before it resolves*, and both players see the same board
mid-resolution. Parcello takes this for the sealed bid: `BlindAuctionResolved`
flips every seat's bid face-up **at once, on their markers**, and holds -
the hold is what makes a simultaneous decision comparable.

**Teamfight Tactics - the shared clock.**
TFT's players all act at once against one visible timer, and the game is
readable because the timer is *one object everyone shares*. Parcello's
sealed-bid and bribe windows are exactly this shape (ADR-0018's timed
collection window), and take the same treatment: the window clock is a
single shared object, drawn on the contested tile.

**Balatro - numbers as spectacle, and what makes it work.**
Balatro's escalating counters are the entire dopamine loop, and the trick is
that the number's *growth is legible in stages* - you see each multiplier
apply. Parcello takes this narrowly and deliberately: the rent chit **grows
as it passes over a boosted tile**, so a big number is *earned in front of
you* rather than delivered. That is one borrowing, in one place. Parcello is
not a numbers-go-up game and must not become one.

**Mini Motorways / Dorfromantik - the calm.**
Both are proof that a game can be *entirely* readable with a tiny visual
vocabulary and no effects at all, and that restraint reads as quality. They
also both animate *state changes only* - nothing moves for decoration, ever.
Parcello takes: the small alphabet (section 4), the no-decoration rule, and
Dorfromantik's specific trick of using **a soft settle to confirm a
commitment** - which is what a placed building should feel like.

**The synthesis, in one line:** *Into the Breach*'s board discipline, with
*Hearthstone*'s sense of weight on the one action you take every turn, and
*Mini Motorways*' restraint everywhere else.

---

## 12. Decisions and trade-offs

| Decision | Why | What it costs |
| --- | --- | --- |
| **Fixed camera, always** | Competitive spatial memory; peripheral readability | No cinematics, no dramatic zoom on the win. The win is sold by *recede* + stillness instead. |
| **No bounce/spring anywhere** | Art Deco register; bounce reads as casual | The game feels "quieter" than a mobile title. That is the intent, and it is a real trade. |
| **Money travels; it is never a delta** | Makes "who paid whom" free; fixes rent being invisible to the earner | Costs ~500 ms per transfer and one more moving object on screen. Worth it. |
| **Tiered budget (8/6/4 s per Update), enforced by compiling first** | The 10 s server cap is a hard wall; overrunning it desyncs the client. Tiering it along the tiers spends the time where a player actually needs it | Long card chains get truncated. The alternative - being behind the game - is strictly worse. Raising the budget forced raising `ANIM_ACK_CAP` with it: the two are one contract. |
| **Coalescing over enumeration** | An 8-tile bankruptcy is *one* event, not eight | Individual tile transfers are less individually visible. They are still in the log. |
| **Tier is per-observer** | Being attacked must never be ambient | Slightly more compile-time logic; a shared Plan cannot be broadcast identically to all seats (it never could - ADR-0007 already makes trades private). |
| **Instant profile is a first-class path** | Accessibility; and it is the path ADR-0028 already guarantees the server tolerates | Every beat must be written so its `apply()` is meaningful with zero duration. This is a real constraint on how beats are authored, and it is a good one. |
| **A 15-field fiche per event was rejected** | 43 x 15 is unmaintainable and would rot within a month | Some per-event nuance lives in prose (8.2) rather than in a cell. Policies (skip, reconnect, simultaneity) are stated once and apply universally - which is what makes them a *grammar*. |

---

## 13. What is built, and what is not

This document is the design. Not all of it is code yet. Keeping the two apart
is the point of writing it down - a spec that quietly pretends to be an
implementation is worse than no spec.

**Built (2026-07):**

| | Where |
| --- | --- |
| Design tokens - the whole `visual-identity.md` palette, sharp corners, muted group colours, the six pawn colours | `lib/tokens.dart` |
| The motion spec - tiers, lanes, profiles, budget, every duration and curve | `lib/motion.dart` |
| Transient stage state, anchors, chits, the three attention devices | `lib/stage.dart` |
| The compiler - pure `compile()`, the budget rule, coalescing, per-observer money | `lib/director.dart` |
| Travelling chits and the P1 arrest | `lib/overlay.dart` |
| Board: real palette, recede/lift/frame, threat flash, band sweep, refusal shake, market-adjusted prices | `lib/board.dart` |
| Sealed-bid reveal on the seat markers; the motion-profile knob; Escape to skip | `lib/main.dart` |
| 24 tests: the budget invariant, coalescing, the truth rule, the money rule, the render path | `test/director_test.dart`, `test/stage_render_test.dart` |

Beats now compiled: movement card, move, jail hop, card reveal, every money
path (salary, rent, tax, card cash, build, boost, mortgage, redeem,
liquidation), auction open, **auction resolve with every bid face-up**, band
sweep, expropriation, the sprung boost trap, spotlight, market event,
bankruptcy, and all five win conditions.

**Specified here, not yet built** - the honest list:

- The sealed-bid input is still in the centre panel. The beat lifts the tile
  and recedes the board, but the *input does not anchor to the tile*, and the
  window clock is still a number in a corner rather than a hairline draining
  along the tile's own edge (section 8.2). This is a layout change, and it is
  the single biggest remaining gap between this document and the build.
- Trade animations (`TradeProposed`/`Accepted`) are still log-only.
- The bribe vote reveals as a banner, not as votes flipping face-up.
- The contested-win discount does not yet shrink the chit in flight
  (`BidReveal.discounted` is computed and unused).
- The AFK auto-play marker, the hand-refill beat, the bot-thinking pulse and
  the time-bank P2 alarm (section 8.4) are not implemented.
- Reconnect snaps to truth (the correct half) but does not re-orient
  (section 9).
- Fonts are not bundled: Fraunces / Inter / Source Serif 4 are specified in
  `visual-identity.md` and the client still renders in Material's default.
- The board is still flat, not isometric.

## 14. Future work

- **Isometric board** (`visual-identity.md`): the motion primitives here are
  all defined as origin -> destination on board objects, so they survive a
  projection change. Deliberately no primitive depends on the board being
  flat.
- **Audio pass.** The SFX set is currently a placeholder and contains a
  literal contradiction: `sfx.diceRoll()` fires on `MovementCardPlayed`, and
  Parcello has had no dice since ADR-0017. Sound must become earcons
  (one identity per *category*, per section 4), not clips per action.
- **Haptics** on the Steam Deck build (a P1 arrest and a boost trap are
  exactly what haptics are for).
- **Spectator profile**: no decisions, so every P2 demotes to P3. Falls out
  of the tier system for free.
- **Replay**: the accepted-command log already replays bit-identically
  (ADR-0001). A replay viewer is a director fed from the log instead of a
  socket - the architecture in section 10 makes this a data-source swap, not
  a feature.
