# Player Behavior

The product-level model of **what the player actually does during a game, and
what the player actually does, how they make decisions, and how their attention moves during play -- the player's behaviour and cognitive process during play** - independent of any screen, layout, component, or
implementation. Written so that a team (or an AI) with no project history could
reconstruct the same play experience, and so that it survives a complete UI
rewrite unchanged.

## Scope

The observable and cognitive behavior of a player in a live game: the actions
they take, the moments they live through, and the trajectory of their attention
within each moment - as product truth, not as interface.

## Responsibilities (what this document owns)

- The player's **real activities**, moment by moment, including activity that
  happens outside their own turn (the game pulls the player into simultaneous,
  timed decisions).
- The **attention loops**: for each recurring moment, its **trigger**, the
  **order of attention** (what the player consults, in sequence), the
  **approximate duration**, and the **cognitive goal**.
- The **common patterns** across those loops (recurring anchors, pivots, and
  couplings in the player's attention).

## Exclusions (what this document NEVER contains)

- No placement, geometry, layout, or screen organization.
- No components, widgets, styles, colours, typography, or icons.
- No motion/animation timings, curves, or audio (feedback specification is a
  separate concern and lives elsewhere).
- No "feelings" as such - the emotional-journey lens is a separate concern.
- No rule definitions - the rules live in the game documentation; this file
  describes the behavior that ARISES from those rules, never the rules
  themselves.
- No balance discussion.
- No information-requirements model - the resulting inventory, permanent-vs-
  contextual classification, and consulted-together grouping are a separate,
  sibling concern.
- No implementation detail of any kind.

---

# What a player actually does during a game

A Parcello game is short and fast, and the player is almost never idle. Two
mechanics make sure of it: an unowned property that someone lands on goes to a
**sealed bid that everyone may enter at once**, and a jailed player's bribe opens
a **vote that every opponent decides at once** - so the player is pulled into
timed decisions *during other players' turns*, not only their own. Everyone's
money, everyone's movement cards, and the coming market events are public; the
only hidden thing is a bid in flight. So the player is never gathering
information in the dark - they are racing a clock over facts already on the
table.

Across every loop, a player's attention lands on a small, recurring set of
things: **the board** (who is where, who owns what), **a focused property** (what
one specific tile is worth and its state), **the standings** (every player's
money and progress toward winning, and who is close), **their own money**, **their
hand** of movement values, **the decision in front of them right now**, **the
clock** (how much time is left in the current turn or window), and **the
consequence that just happened** (money moving, a card played, a result). The
loops below describe the ORDER in which the player consults these, not where any
of them sits.

## Normal turn (moving)

- **Trigger:** it becomes the player's turn to act.
- **Activities:** they orient on the board, weigh the movement values in their
  hand against where each would land, play one value, watch the pawn travel and
  arrive, and - if the turn allows - act on the property they hold before ending
  the turn. A turn clock and a personal time reserve run throughout.
- **Order of attention:** clock (how long do I have) -> board (where am I, what
  surrounds me) -> hand against the board (which value lands where), oscillating
  between the two -> the chosen play -> the consequence (the pawn moves and
  lands) -> the tile arrived on -> a quick pass over the standings -> back to the
  board.
- **Approximate duration:** a few seconds to about fifteen.
- **Cognitive goal:** "where do I want to be next, and which of my values gets me
  closest?" The defining act is judging the hand against destinations.

## Auction (bidding)

- **Trigger:** a landing puts an unowned property up for a sealed bid; a fixed,
  short window opens; every living player may bid at once. This fires on other
  players' turns as often as on the player's own.
- **Activities:** they identify the tile, judge what it is worth to them (does it
  complete a set, does winning it deny a rival), check what they can afford,
  decide whether and how much to commit, possibly adjust the amount quickly, and
  submit or abstain before the window closes - then watch every bid reveal at
  once and the property settle.
- **Order of attention:** the consequence/board (which tile, someone landed) ->
  the focused tile (what is it worth) -> their own money (can I afford it) -> the
  standings (who else wants it, whom am I denying, who is close to winning) -> the
  decision (compose the amount), ping-ponging with the clock (how long is left)
  -> submit -> the consequence (all bids revealed together, held to compare) ->
  the standings (who won, money moved).
- **Approximate duration:** the fixed short window - on the order of ten-odd
  seconds - under real pressure.
- **Cognitive goal:** "is this worth a price I can pay, given whom I am blocking,
  before time runs out?" This is the fullest and fastest evaluation in the game,
  and the player performs it off-turn as much as on-turn.

## Trade (negotiating)

- **Trigger:** the player decides to propose an exchange, or an exchange
  proposed to them arrives. It may happen at almost any time and carries no hard
  window of its own.
- **Activities.** Proposing: they decide what they need and from whom, value the
  properties and cash on each side, assemble an offer, send it, and keep playing
  while it sits pending. Responding: they read what is offered and asked, judge
  the net effect on themselves, judge the effect on the proposer's position, then
  accept, decline, or counter.
- **Order of attention.** Proposing: the standings (who holds what I need, whom I
  want to slow) -> the board and focused tiles (which properties, their worth) ->
  their own money (what I can add) -> the decision (assemble) -> send. Responding:
  the arriving offer -> the focused tiles (worth of each side) -> their own money
  (net effect) -> the standings (does accepting hand the other player a lead) ->
  decide.
- **Approximate duration:** variable and unhurried - seconds to tens of seconds;
  the only common decision with no hard window, though the turn and game clocks
  keep running.
- **Cognitive goal:** "does this exchange advance MY position more than my
  counterpart's?" The check against the standings dominates, because an exchange
  that looks fair but lets a rival win is a loss.

## Construction (improving)

- **Trigger:** it is the player's turn, in the phase where they may develop the
  property they hold.
- **Activities:** they survey what they own, consider what an improvement adds
  and costs on a candidate, check affordability and whether a shared building
  supply can cover it, consider whether developing now raises their standing or
  risks triggering an end-of-game condition, apply the change, and repeat or stop
  before ending the turn.
- **Order of attention:** the board and their own holdings (which of my
  properties) -> the focused tile (what an improvement adds and costs) -> their
  own money (can I, and is the supply available) -> the standings (does this raise
  my score, does it risk ending the game) -> the decision (build or sell) -> the
  consequence (money leaves, the property changes) -> back to the board.
- **Approximate duration:** a few seconds to about twenty.
- **Cognitive goal:** "which improvement most raises my income and standing for
  the money, without emptying a shared supply I would rather keep?"

## Vote (deciding on a bribe)

- **Trigger:** a jailed player offers a bribe to escape; a very short window
  opens; every eligible opponent decides at once. It fires off-turn.
- **Activities:** they register who is bribing and how much, judge whether
  freeing that specific player helps them (is that player a threat, is that
  player close to winning), weigh the share they would receive if the bribe
  passes, and choose accept or reject before the window closes.
- **Order of attention:** the decision/consequence (who is bribing, how much) ->
  that player's place in the standings (do I want this one free) -> their own
  money (my share if it passes) -> the clock -> decide.
- **Approximate duration:** the shortest window in the game - a few seconds -
  and binary.
- **Cognitive goal:** "does my cut outweigh the cost of freeing this particular
  rival, right now?" The shortest chain, and the most pressured per second.

## Jail (deciding how to get out)

- **Trigger:** the player is imprisoned and it becomes their turn to choose how
  to leave.
- **Activities:** they recognize they are jailed, weigh the exits open to them
  against their situation (a guaranteed exit they already hold, buying freedom
  from opponents which must then be voted, or committing to a route out that
  carries a lasting cost), choose one, and commit under the turn clock.
- **Order of attention:** their own situation and resources (which exits do I
  have) -> the standings and board (is spending to leave now worth it, given the
  race) -> the decision (choose, and if buying freedom set an amount), against the
  clock -> commit -> the consequence.
- **Approximate duration:** a handful of seconds under the turn clock.
- **Cognitive goal:** "what is the cheapest exit that does not cost me the race,
  and can I get it accepted?" A one-off decision that briefly turns the player
  from a mover into a negotiator.

## End of game (the result)

- **Trigger:** a decisive event ends the game - one player left standing, someone
  reaches the winning score, a shared supply runs dry, or time expires - and the
  table stops. A player defeated earlier has already shifted from acting to
  watching the rest of the game.
- **Activities:** they register what just happened and who won, locate themselves
  in the final standings, and then decide what to do next - play again with the
  same table, or leave.
- **Order of attention:** the consequence (the result - who won, or who fell) ->
  the standings (where did I finish) -> the result detail -> the decision (again,
  or out).
- **Approximate duration:** a held moment, then an unhurried result the player
  leaves on their own timing.
- **Cognitive goal:** "what just happened, where did I land, and do I want
  another?"

# Common Cognitive Patterns

These are the structures that recur underneath almost every loop above - the
invariants a rebuilt interface would have to preserve to reproduce the same play.

1. **The board is the origin and the return.** Every loop begins with something
   on the board and ends by coming back to it. The player leaves the board only
   to evaluate and returns the moment the evaluation is spent; the board is the
   resting state of attention and everything else is a brief departure from it.
   The player's own eye supplies the movement, which is why the game itself can
   hold still.

2. **One evaluation runs under almost every decision: worth, then affordability,
   then consequence-for-the-race.** Bidding, trading, building, and voting are the
   same three-beat judgment in different clothes - "what is this worth to me / can
   I pay it / what does paying it do to who is winning." The loops differ in their
   trigger and their speed, not in the shape of the thought. Serve that one
   judgment well and every priced decision in the game is served.

3. **The player's own money is the pivot.** After the board, nothing is consulted
   as often. It sits in the middle of every decision - between worth and the race,
   or between an offer and an answer - because in a game where every action is
   priced, the first question of any choice is "against what I hold." It is re-read
   continuously, not fetched once.

4. **The standings are consulted as a move, not as a scoreboard.** The player
   reads who is winning not to keep score but to decide whether to deny a bid,
   refuse a trade, or spend to escape. The standings are an input to the decision
   in progress, read in the same thought as it. Because the game can end the
   instant a player reaches the target, "who is about to win" is never trivia -
   it is a reason to act now.

5. **Time pressure tightens an existing couple rather than adding a step.** In the
   untimed decision (a trade) the player barely consults the clock; in the
   shortest ones (a vote, a bid) their attention alternates between composing the
   choice and reading the window drain. The steps of the decision do not change
   with the clock - the coupling of the decision and the time left simply draws
   tighter as the window shortens, until the two are read together as one.

6. **The player is almost never idle, and acts most often when it is not their
   turn.** The auction on every landing and the vote on every bribe pull the
   player into fast, priced decisions during other players' turns; the single
   most frequent decision in the game - the sealed bid - is an off-turn decision.
   Attention is therefore never parked waiting for a turn - it is always either
   deciding or monitoring for the next thing to decide.

7. **The whole profile follows from one difference: speed over shared facts.**
   What separates Parcello, behaviorally, from a classic property game is that
   almost nothing is hidden and the clock is always short. A classic property
   game rewards patience and slow accumulation, and its player waits through the
   turns of others; Parcello's player reads a fully lit table and commits faster
   than the rivals, on and off their own turn. Every other pattern here - the
   constant monitoring, the off-turn decisions, the money-as-pivot, the tightening
   decision/clock couple - is a consequence of that single fact: the information
   is already on the table, and the game is a race to use it.
