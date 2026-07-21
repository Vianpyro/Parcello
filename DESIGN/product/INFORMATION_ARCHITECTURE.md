# Information Architecture

The product-level model of **the information a player consults during a game,
and the requirements any interface must satisfy** to present it - independent of
any screen, layout, component, or implementation. Written so that a team (or an
AI) with no project history could reconstruct the same play experience.

## Scope

The information model of a live game: what information exists, when it becomes relevant, and which pieces are evaluated together, when, and
which pieces are read together - expressed as **requirements** an interface must
meet, never as an interface itself.

This document models product truth, not implementation. If implementation and this document disagree, the implementation is wrong or the product decision has changed.

## Responsibilities (what this document owns)

- The **inventory** of every piece of information the player consults during a
  game.
- The classification of each piece as **permanent** (must remain available
  throughout) vs **contextual** (belongs to a specific phase and then leaves).
- The **consulted-together groups**: which pieces of information the player
  reads in one thought (cognitive adjacency, not spatial adjacency).
- The **priority / salience** ordering, especially under time pressure, and the
  set that **must never be hidden**.

## Exclusions (what this document NEVER contains)

- No placement, geometry, spatial adjacency, layout, or any location.
- No screen-by-screen organization - the per-screen application is a separate,
  downstream concern.
- No components, widgets, styles, or motion.
- No behavioral narrative - what the player DOES and where they look is a
  separate, sibling concern; this file states the resulting information
  requirements only.
- No implementation detail of any kind.

---

# Information Inventory

Every distinct piece of information a player draws on during a game. Each entry
names WHAT the information is. Nothing here says how much of it is needed at
once, when, or where - those are the sections below, and, for arrangement, no
part of this document.

**Shared game state**
- Each player's current location on the property track.
- Each property's owner (or that it is unowned), and its development and
  mortgage state.

**The players (all of them, the player included)**
- Each player's current money.
- Each player's progress toward the winning condition, and how close each is to
  reaching it.

**The self**
- The player's own money (the same figure named under the players, and the one
  drawn on most).
- The movement values the player currently holds.
- The properties the player owns.

**A property under evaluation**
- What one specific property is worth: the payment it currently commands,
  whether owning it completes a set, its development and mortgage state, and its
  price when it is being acquired.

**The active decision**
- The choice currently open to the player, and the quantity that choice turns on
  (an amount to commit, a cost to pay, the two sides of an exchange, a yes/no).
- Which other player, if any, the choice is made against or with (a rival being
  outbid, a counterpart in an exchange, the person asking to be freed).
- The options available for the choice, and the option to decline it.

**The clock**
- Whose turn it is, and which phase or timed window is currently active.
- How much time remains in the current turn or window.

**The game's horizon (how the game can end)**
- The winning target and each player's distance from it (a facet of the players'
  progress named earlier).
- How close a shared, exhaustible resource is to running out.
- The publicly known schedule of upcoming events that will change what
  properties are worth.

**The most recent consequence**
- What value just moved and between which parties, or what result was just
  produced.

**The result (once the game ends)**
- Who won and by which condition, the final ordering of all players, and where
  the player themselves finished.

---

# Permanent Information

Information that is relevant at every moment of a live game, because the player
can be required to make a decision drawing on it at any time - including during
phases that occur on other players' turns. It must remain available continuously;
recovering it may never require the player to leave a phase, wait, or take an
action.

- **The shared game state** - every player's location and every property's
  ownership and development.
- **The full standings** - every player's money and every player's progress
  toward the winning condition, including who is closest. Because the game can
  end the instant a player reaches the target, each player's distance to that
  target belongs to this permanent set, not to any single phase.
- **The player's own money.**
- **The player's own resources** - the movement values held and the properties
  owned.
- **The time context** - whose turn or which timed window is active, and how
  much time remains in it.
- **The game's horizon** - the winning target, the level of any shared
  exhaustible resource whose depletion ends the game, and the known schedule of
  upcoming events that change valuations.

A piece is permanent when its absence would harm a decision the player could be
required to make at an unpredictable moment. All six items listed here meet that
test.

---

# Contextual Information

Information that is relevant only during one phase and ceases to be relevant when
that phase ends. It must be available for exactly the span of its phase, and must
not be presented as if it persisted.

- **A property's full valuation** - the detailed worth and state of one specific
  property, relevant while that property is being evaluated (acquired, charged
  by, valued in an exchange, developed, or inspected) and not otherwise.
- **An open sealed-bid contest** - the property at stake, the price that bounds a
  valid commitment, the amount the player is committing, the available ways to
  raise it, and the option to commit nothing. The amounts other players are
  committing are NOT available while the contest is open; they become available
  only at the single simultaneous moment that closes it.
- **The sealed-bid outcome** - every player's committed amount, the winner, and
  the price paid - available only at that closing moment, and only briefly.
- **An open bribe vote** - who is asking to be freed, the amount offered, the
  share the player would receive if it passes, and the accept/reject choice -
  available only while the vote is open.
- **An exchange under consideration** - the two sides of the proposed exchange
  and the accept / decline / counter / withdraw options. An exchange is private
  to its two parties; its terms are available to no one else.
- **Development choices for a property** - what a development step adds and costs,
  whether a shared building resource can currently supply it, and the build/undo
  choice - relevant only while the player is developing.
- **Escape choices when jailed** - the exits available to the player and the cost
  of each - relevant only while the player is imprisoned and choosing.
- **A takeover option** - the ability, and cost, to take a specific rival
  property the player has just reached - relevant only at the point of the
  player's turn when it is offered.
- **The most recent consequence** - what just moved or resolved - relevant
  briefly, then no longer.
- **The end result** - who won and how, the final ordering, and where the player
  finished - relevant only once the game has ended.

Two contextual items carry a hard secrecy requirement: a sealed bid in flight,
and the terms of an exchange, belong only to their owner (the closing moment for
the first, the two parties for the second). Making either available earlier or
wider is wrong, not merely imperfect.

---

# Consulted-Together Groups

Sets of information the player resolves as a single judgment. The requirement is
that the player be able to complete each judgment without losing access to any
member of the set for the span of that judgment. These are cognitive groupings;
they say nothing about arrangement.

- **Cost against means** - the quantity a choice turns on (an amount, a cost, an
  offer) together with the player's own money. Every priced choice is this one
  comparison.
- **The valuation judgment** - a property's worth, the player's own money, and
  the standings, resolved as one assessment: what it is worth, whether it can be
  paid, and what paying does to who is winning. This is the recurring judgment
  under acquiring, exchanging, and developing.
- **Choice against the clock** - the active decision together with the time
  remaining in its window, resolved as one under any timed decision.
- **The stake against the standings** - the specific player a decision is made
  against or with, together with that player's standing (their money and their
  nearness to winning). Outbidding, refusing, or freeing someone is judged with
  that person's standing in mind.
- **The consequence and its effect** - a just-resolved event together with the
  change it produced in the standings, resolved as one to understand the outcome.
- **Means against destinations** - the player's own movement values together with
  the locations they would lead to, resolved as one when choosing how to move.

---

# Priority Under Time Pressure

The shorter the active window, the smaller the set of information a player can
resolve before deciding. The requirement follows: a timed decision must be
completable from a minimal set alone, and that minimal set must be available for
the whole window.

- For the shortest timed decisions (a bribe vote, a sealed bid), the minimal set
  is: the choice and the quantity it turns on; the player's own money; the single
  other party or stake the choice is made against; and the time remaining. A
  correct decision must be reachable from these alone.
- The broad permanent information (the full standings, the game's horizon, the
  whole shared state) is reference resolved BETWEEN timed windows, not within
  them. A timed decision may therefore never depend on re-resolving the broad
  set; anything a timed decision truly needs must belong to that minimal set.
- Order of need, tightest window first: the decision and its quantity; then the
  player's own money; then the single implicated party or stake, and the time
  remaining; then everything else. As the window lengthens - up to an untimed
  exchange - the player resolves more of the inventory and this collapse relaxes.

---

# Information That Must Never Be Hidden

These guarantees hold at every moment; none may be suppressed, deferred, or made
to require an action, however briefly.

- **The time context** - whose turn or window is active, and how much time
  remains. A concealed clock silently spends a decision.
- **Every player's money and nearness to winning.** In a game that can end the
  instant a player reaches the target, concealing how close anyone is lets a
  player fall out of contention without warning.
- **The player's own money.** Every choice is priced against it.
- **The shared game state** - locations and ownership.
- **Any decision the player is currently eligible to make, and what it
  concerns.** A window the player could act in but was never given is a decision
  taken from them.
- **The game's horizon** - the winning target, the depletion level of a
  game-ending shared resource, and the known schedule of events that change
  valuations. The game's premise is that these facts are shared; a player denied
  them plays a poorer game than the others.

The converse is the same requirement read from the other side: the two secret
items named under Contextual Information - a sealed bid in flight and the private
terms of an exchange - must never be made available beyond whom they belong to.
"Never hidden" and "never wrongly disclosed" are one contract.
