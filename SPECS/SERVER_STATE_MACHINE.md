# Server Game State Machine - specification

A SPECS document (see `SPECS/README.md`): observable functional behaviour only.
It specifies the authoritative server-side state machine of one game: its states,
transitions, the clocks that drive it, the concurrency model, its guarantees, and
its impossible states. It describes behaviour any implementation must satisfy; it
names no framework, type, or mechanism. It complements `SPECS/game/GAME_SCREEN.md`
(the client-facing surface), `SPECS/EVENTS.md` (the facts), and
`SPECS/INFORMATION.md` (the data).

The machine is **two nested levels**:

- **Room phase** (outer): the room's life around a game.
- **Turn phase** (inner): the engine's cycle WITHIN a live game.

A "coherence with the engine" section at the end records that the states and
constants named here were verified against the Rust engine and server, not
assumed.

---

# States

## Room phase (outer)

- **Lobby** - the room is being assembled; no game exists yet; settings are
  editable (see `SPECS/game/LOBBY_SCREEN.md`, `SETTINGS.md`).
- **Active** - a live game is in progress; the game state exists; settings are
  frozen.
- **Finished** - the game has ended; the final game state is retained (for the
  result and for a possible replay in the same room).
- **(Dissolved)** - a terminal, out-of-band state: the room ceases to exist. It
  is not a phase the game passes through; it is the room's disappearance.

## Turn phase (inner, only while Active)

- **AwaitMove** - waiting for the acting seat to play a movement value, or - while
  jailed - to choose an exit (route, bribe, or jail card).
- **BlindAuction { tile, one bid slot per seat }** - a landing on an unowned
  property opened a sealed-bid window; every living seat may commit once.
- **BribeVote { briber, amount, one vote slot per living opponent }** - a jailed
  player's bribe opened a simultaneous vote among living opponents.
- **AwaitEnd** - movement resolved; development is allowed; waiting for the acting
  seat to end the turn.

There is exactly one turn phase at any instant of a live game.

---

# Transitions

## Room phase

```
Lobby --host starts (>=2 seats, valid settings)--> Active   (settings frozen; engine built; start seed drawn)
Lobby --automatic start (a matched room)---------> Active
Lobby --start with invalid settings--------------> Lobby    (refused; unchanged)
Lobby --last participant leaves / idle-----------> Dissolved
Active --a terminal event fires------------------> Finished
Active --game clock expires-----------------------> Finished (richest by net worth wins)
Finished --a still-connected seat replays---------> Active   (same room restarts for connected seats)
Finished --ranked: replay goes through the queue--> Dissolved (this room ends; players re-queue)
Finished/Active --no connected seat, idle limit---> Dissolved
```

## Turn phase (within Active)

```
AwaitMove --movement played, ordinary landing-----> AwaitEnd
AwaitMove --movement played, unowned-property land-> BlindAuction
AwaitMove --jailed: bribe offered-----------------> BribeVote
AwaitMove --jailed: route/jail-card exit----------> (resolves, then) AwaitEnd
BlindAuction --window closes / all committed------> (resolve) --> AwaitEnd   (auction is part of the landing)
BribeVote --window closes / all voted-------------> (resolve) --> AwaitEnd (or the turn ends on failure)
AwaitEnd --end of turn----------------------------> AwaitMove (next living seat)   [TurnStarted]
AwaitEnd --a development ends the game-------------> (terminal) --> Finished
```

The acting seat is stable for the whole of a `BlindAuction` it opened (the turn
does not advance until the window resolves). `BlindAuction` and `BribeVote` are
the only multi-actor phases; every other phase has a single acting seat.

---

# Events

Transitions are driven by the events catalogued in `SPECS/EVENTS.md` - accepted
player commands and system facts. This document does not restate them; it names
the ones that MOVE the machine: a movement play, a bid submission, a vote, an
end-of-turn, a build, an elimination, a terminal condition (outer), and the
window-close and turn-advance facts the clocks produce (below). A rejected
command produces no event and no transition.

---

# Clocks

The machine is driven by a set of independent, armed deadlines. Two kinds:

- **Per-game durations** (from the frozen settings): the overall game clock, the
  per-turn clock, and the personal time bank. Any may be OFF.
- **Fixed server constants** (server behaviour, not game rules): a disconnect
  grace, an animation-acknowledgement cap, a bot think-time, and a room idle
  limit.

| Clock | Armed when | Fires -> | Gated on render ack | Absolute |
|---|---|---|---|---|
| **Game clock** | a time-boxed game is Active | conclude the game, richest by net worth wins (-> Finished) | **No - never gated** | **Yes** |
| **Turn / AFK clock** | a single seat is acting (AwaitMove/AwaitEnd) and a turn limit is set, or the acting seat is disconnected past its grace | auto-play the canonical action for that seat | Yes | No |
| **Time bank** | the acting seat's plain turn window elapsed | drain the seat's reserve, then hard-stop the turn | Yes | No |
| **Bid window** | phase is BlindAuction | resolve the auction; silent seats auto-abstain | Yes (table-wide) | No |
| **Vote window** | phase is BribeVote | resolve the vote; silent opponents auto-reject | Yes (table-wide) | No |
| **Disconnect grace** | an acting seat's connection is down | after the grace, the seat's turns are auto-played | n/a | No |
| **Bot think-time** | the acting seat is a bot | the bot's chosen action is applied | Yes (table-wide) | No |
| **Idle limit** | no connected participant remains | dissolve the room | n/a | No |

**The armed state of every clock is DERIVED from the current phase on every
cycle, never stored** - a phase change re-arms exactly the deadlines that phase
warrants (a bid window is armed only during BlindAuction, and disarmed the moment
it resolves).

## The render-acknowledgement watermark

Every state update the server broadcasts carries a monotonic sequence number;
clients acknowledge "rendered through N". The turn, bank, bid, vote, and bot
clocks WAIT for the relevant acknowledgement (the acting seat's, or the whole
table's) before they count down - so animation on the client never eats a
player's thinking time. This wait is bounded by the **animation-ack cap**: a
client that never acknowledges (a bug, a throttled background, malice) can delay
a window only up to the cap, after which the clock proceeds regardless. The
**game clock is the sole exception - it is never gated**, because the absolute
time limit must not be extendable by withholding acknowledgements.

---

# Concurrency

- **One serial worker per room.** A room is processed by a single sequential
  worker; there is no concurrency WITHIN a room. Every incoming command and every
  clock firing is handled one at a time, in a definite order. Rooms are
  independent of one another and run concurrently only across rooms.
- **The worker races input against the clocks.** On each cycle it waits for
  whichever comes first: an incoming command, or the earliest armed deadline. A
  fired clock is handled exactly like a command - it injects an auto-action
  (auto-play, auto-abstain, auto-reject, a bot move, or the game conclusion) into
  the same serial stream.
- **Serialization is what makes the game deterministic.** Because commands (and
  clock-injected auto-actions) take effect in one definite order, the engine's
  state is a pure function of the initial participants, the seed, and that
  ordered stream - the replay guarantee (`SPECS/INFORMATION.md`,
  `SPECS/EVENTS.md`). Concurrency never reorders accepted actions.
- **Simultaneous windows collect, they do not parallelize.** A BlindAuction or a
  BribeVote accepts one contribution per living participant, in whatever order
  they arrive, and resolves as one fact when the window closes or all have
  contributed - still on the single serial worker.

---

# Interruptions

- **A clock firing interrupts waiting**, injecting an auto-action; it never
  corrupts state (an auto-action is a normal, validated action).
- **A terminal condition interrupts a turn**: a development that reaches the
  victory target or empties the shared pool, or the game clock expiring, ends the
  game immediately - the current turn does not complete, the phase becomes
  Finished, and no further game action is accepted.
- **A disconnection interrupts an acting seat**: after the grace, its turns are
  auto-played; the game never blocks on an absent player.
- **A window is never interrupted by a new turn**: the turn does not advance until
  the window resolves; the two never overlap.

---

# Guarantees

- **Rejections never mutate.** A refused command leaves the game state untouched
  and is reported only to its issuer; no partial application exists.
- **Deterministic replay.** The initial participants, the seed, and the ordered
  accepted actions reproduce the entire game identically; the only value not
  reproduced is the real-time clock reading (`SPECS/INFORMATION.md`).
- **Settings frozen at Active.** The configuration is copied into the game at the
  Lobby -> Active transition and never changes thereafter.
- **One timed window at a time.** BlindAuction and BribeVote never coexist; while
  a bid window is open, no cash may change and no trade may resolve (the auction
  solvency invariant).
- **The game clock is never gated.** The absolute time limit cannot be extended by
  withholding render acknowledgements.
- **Per-room serialization.** No two actions in a room take effect at the same
  instant; the order is definite and total.
- **Deterministic dissolution.** A room dissolves exactly when no participant
  remains or the idle limit elapses.
- **The turn never waits forever.** Every phase with an actor has a deadline (a
  turn/bank clock, a window clock, a grace) whose expiry produces a defined
  default action, so no phase can stall indefinitely.

---

# Impossible states

- Two turn phases at once; a live game with no turn phase.
- A bid outside a BlindAuction, or a vote outside a BribeVote.
- Any cash change, or any trade resolution, while a BlindAuction is open.
- A BlindAuction and a BribeVote open at the same time.
- The turn advancing while a window it opened is still open.
- Settings changing while Active or Finished.
- Any game action accepted after Finished (only a replay restarts play).
- Active reached from Finished without a replay; Active reached from anything but
  a start.
- A clock armed for a phase that is not current (a bid deadline while AwaitMove).
- A game action taking effect concurrently with another in the same room.
- The game clock being paused or extended by a client withholding acknowledgement.

---

# Coherence with the Rust engine (verified, not assumed)

Checked against the engine and server sources:

- **Inner phases are exactly the engine's turn phases:** `AwaitMove`,
  `BlindAuction { tile, bids }`, `BribeVote { briber, amount, votes }`,
  `AwaitEnd` - the four verified variants, matching this document one-to-one.
- **Outer phases are exactly the room's phases:** `Lobby`, `Active(state)`,
  `Finished(state)` - three variants, matching.
- **The clocks named here exist as the server's armed deadlines:** the game
  deadline, the bid and vote deadlines, the derived per-turn/AFK deadline, the
  time bank, plus the fixed server constants (a disconnect grace of thirty
  seconds, a bot think-time under a second, an idle limit of thirty minutes, and
  an animation-acknowledgement cap of ten seconds). Their armed state is
  recomputed from the phase every cycle, never stored.
- **The render-ack watermark is the ADR-0028 mechanism:** updates carry a
  monotonic sequence, clients acknowledge through a sequence, and the turn/bank/
  bid/vote/bot clocks wait on it up to the cap while the game clock does not.
- **Serial per-room processing** matches the single-worker-per-room model, and it
  is exactly what upholds the engine's deterministic-replay invariant.

Any divergence between this document and those sources is a bug in one of them;
this document is the functional statement, the engine is the authority.

---

# Out of Scope

This document never describes, and a reader must never infer from it:

- any interface, control, layout, colour, typography, animation, or sound;
- any framework, runtime, concurrency primitive, type, or field name;
- any wire format or storage schema;
- rule definitions or numeric game values (owned by the rules and engine
  documents; the fixed SERVER constants above are named as server behaviour, not
  game rules);
- the client-facing surface behaviour (owned by `SPECS/game/*`).

---

# Phase 4 - Self-critique

- **This layer merits existing, and it was un-owned.** The authoritative
  server-side machine - the room phase around a game, the engine's turn phase
  within it, the clock set, the render-ack gating, and the per-room
  serialization - lived only in the engine and server sources and scattered ADRs.
  No document stated it as one machine with its guarantees and impossible states.
  Created as `SPECS/SERVER_STATE_MACHINE.md`.
- **Verified, not assumed.** The states and the constants were read from the
  engine's `TurnPhase`, the room's `Phase`, and the server's deadline set and
  constants, then restated functionally. Where a source comment and the ruleset
  disagree on a window's exact duration, I named the window without a number, so
  the document cannot inherit a stale figure - the durations that are settings are
  frozen-at-launch, and the fixed server constants are named as behaviour.
- **Boundary honesty (behaviour, not mechanism).** The document says "one serial
  worker per room", "armed deadline", "render acknowledgement" - functional
  descriptions - never the runtime, the primitive, or the types. A reimplementation
  in any language could satisfy it. The one deliberate exception is the coherence
  section, which cites the real state names precisely because the owner asked for
  verification against the engine.
- **The subtle, load-bearing rule made explicit: the game clock is never gated.**
  Every other clock waits on the render watermark; the absolute game clock does
  not, or it could be extended by withholding acknowledgements. A reimplementation
  that gated all clocks uniformly would be exploitable; the exception is stated in
  the clock table, the watermark section, and the guarantees.
- **Determinism traced to serialization.** The replay guarantee is presented as a
  CONSEQUENCE of per-room serial processing plus the seed, not as a separate
  magic property - which is where a reimplementation would break it (by
  parallelizing a room).
- **Residual risk.** This is the most engine-coupled SPECS document; its states,
  clocks, and impossible states are the server's actual behaviour, so it is the
  one most directly checkable against the running server - a strength (it can be a
  test oracle) and a maintenance duty (an engine change to a phase or a clock must
  update it).
- **Convertibility test.** Every statement is a state, a transition, a clock rule,
  a concurrency rule, or a guarantee; none names a control, a position, or a
  colour. A paper state diagram and the running server would agree on all of it.
