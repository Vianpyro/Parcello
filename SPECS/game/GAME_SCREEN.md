# GameScreen - functional specification

A SPECS document (see `SPECS/README.md`): observable functional behaviour only.
It describes WHAT the GameScreen does, never how it looks. It applies the
cross-surface player behaviour and information model owned by
`DESIGN/product/PLAYER_BEHAVIOR` and `DESIGN/product/INFORMATION_ARCHITECTURE`;
it does not restate them. It must be honourable equally by a paper wireframe and
by a live interface.

Two orthogonal dimensions describe every situation of this surface:

- **MODE** (persistent, the player's standing): `Seated-active`,
  `Eliminated-watcher`, `Spectator`, `Disconnected`, `Reorienting`.
- **CONTEXT** (transient, the current decision): `Monitoring`, `My-move`,
  `My-post-move`, `Jail`, `Auction`, `Vote`, `Incoming-trade`, `Outgoing-trade`,
  `Awaiting-verdict`, `Awaiting-others`, `Result`.

A situation is a `(MODE, CONTEXT)` pair. Timed windows (`Auction`, `Vote`) can
arise regardless of whose turn it is - which is why the two dimensions are
separate.

---

# Purpose

The GameScreen is the only surface on which a player PARTICIPATES in a live
game: reading the shared table, taking the decisions the game presents (during
their own turn AND during other players' turns), and understanding every
consequence, under the game's own clocks. It exists so that a player is, at
every instant, able to (a) monitor the permanent facts, (b) act on the decision
the game currently offers, and (c) understand what just changed - without ever
being shown information the game masks, or being denied a decision they are
eligible to make.

---

# Lifecycle

**Appears** when a game becomes live for the player - the table has finished
assembling and play has begun - whether the player holds a seat or is watching,
including when the player rejoins a game already in progress.

**Disappears** only when the player leaves the game (returning to a prior
surface) or dismisses the ended-game result by choosing a next action. It does
NOT disappear on connection loss, on being auto-played, or on elimination.

**Reactions:**

- **Reconnection.** The surface enters `Reorienting`: it snaps to the true
  present state of the game and presents a single "here is you, here is now"
  orientation. It never replays missed history. It then resolves into whatever
  CONTEXT is currently live (including an open `Auction`/`Vote` the player is
  still eligible for, or `Result`).
- **Disconnection.** The surface enters MODE `Disconnected`: it keeps showing
  the last known state, marked as stale, plus the reconnection status. The game
  continues without the player; after a grace period the player's turns are
  auto-played with the canonical action. None of the player's inputs affect the
  game while disconnected.
- **Abandon (resign).** The player's seat is removed from play; the surface
  transitions the player to MODE `Eliminated-watcher`.
- **Elimination (bankruptcy).** Same destination as abandon: MODE
  `Eliminated-watcher`. The player keeps a public view of the remaining game and
  can only leave (a still-connected eliminated player may still choose to play
  again once the game ends).

---

# Functional States

Functional events the surface receives (named neutrally): `TurnPassed`,
`MovementResolved`, `AuctionOpened`, `AuctionResolved`, `VoteOpened`,
`VoteResolved`, `TradeOffered` (to me), `TradeUpdated` (an offer I am party to
changed), `Consequence` (a value moved / a tile changed that I witness),
`EstateChanged` (my holdings changed), `PlayerEliminated`, `GameEnded`,
`TimeUp`, `ClockTick`, `ConnectionLost`, `Reconnected`, `TurnAutoPlayed`.

Across ALL states, `Consequence`, `EstateChanged`, `PlayerEliminated`,
`ClockTick`, and standings updates keep the permanent information current in the
background; "events ignored" below means only "does not create or change a
decision in this state, nor cause a transition" - never "the permanent
information stops updating".

## CONTEXT states

### Monitoring
- **Trigger:** not the player's turn, and no window open to the player.
- **Exit:** the turn passes to the player; an `Auction`/`Vote` the player is
  eligible for opens; the player initiates a trade; a terminal event.
- **Decisions:** none required; the player may initiate a trade.
- **Events received:** `TurnPassed` (may become My-move/Jail), `AuctionOpened`,
  `VoteOpened`, `TradeOffered` (makes Incoming-trade available), `GameEnded`,
  `ConnectionLost`.
- **Events ignored:** none beyond the background updates above.

### My-move
- **Trigger:** the turn passes to the player, not imprisoned.
- **Exit:** the player plays a movement value (-> `MovementResolved`); the turn
  clock or AFK grace forces the canonical action.
- **Decisions:** play one movement value.
- **Events received:** `MovementResolved` (-> My-post-move, or -> Auction if the
  landing is an unowned property), `TurnAutoPlayed`, `GameEnded`,
  `ConnectionLost`.
- **Events ignored:** `TradeOffered` (an offer becomes available but does not
  interrupt the move); another seat's events cannot arrive (it is the player's
  turn).

### My-post-move
- **Trigger:** the player's move resolved and the phase allows end-of-turn
  actions.
- **Exit:** the player ends the turn; the clock forces the end.
- **Decisions:** develop / undo-develop / mortgage / redeem on owned tiles;
  take over a reachable rival tile (when eligible); end the turn.
- **Events received:** `EstateChanged`, `Consequence`, `TurnAutoPlayed`,
  `GameEnded` (a development may end the game), `ConnectionLost`.
- **Events ignored:** `TradeOffered` (available, non-interrupting).

### Jail
- **Trigger:** the turn passes to the player while imprisoned.
- **Exit:** the player chooses an exit; the clock forces the canonical route.
- **Decisions:** use a held exit; offer a bribe (an amount); commit to a route
  out.
- **Events received:** the exit resolves (-> My-move on success);
  `TurnAutoPlayed`, `ConnectionLost`. Offering a bribe -> `Awaiting-verdict` (and
  opens a `Vote` for opponents).
- **Events ignored:** `TradeOffered` (available, non-interrupting).

### Auction
- **Trigger:** `AuctionOpened` for a landing on an unowned property while the
  player is eligible (alive, not yet committed). Fires on any player's turn.
- **Exit:** the player commits an amount or abstains (-> `Awaiting-others`); the
  window closes.
- **Decisions:** commit an amount (bounded below by the market floor and above
  by the player's money), or abstain.
- **Events received:** `ClockTick` (window), `AuctionResolved` (-> prior
  context), `GameEnded`, `ConnectionLost` (-> silent abstain).
- **Events ignored:** `TradeOffered`/`TradeUpdated` are not actionable while the
  window is open (trade actions are blocked); a pending offer persists,
  suspended.

### Vote
- **Trigger:** `VoteOpened` while the player is an eligible opponent (not the
  briber, not committed).
- **Exit:** the player accepts or rejects (-> `Awaiting-others`); the window
  closes (silence counts as reject).
- **Decisions:** accept or reject.
- **Events received:** `ClockTick`, `VoteResolved` (-> prior context),
  `GameEnded`, `ConnectionLost` (-> silent reject).
- **Events ignored:** trade events (blocked, as in `Auction`).

### Incoming-trade
- **Trigger:** `TradeOffered` addressed to the player exists and is pending.
- **Exit:** the player accepts / declines / counters; the proposer withdraws;
  the offer becomes invalid; an `Auction`/`Vote` opens (the decision is
  suspended, the offer persists).
- **Decisions:** accept / decline / counter.
- **Events received:** `TradeUpdated` (withdrawn / invalidated -> cleared),
  `AuctionOpened`/`VoteOpened` (-> suspended), `GameEnded`.
- **Events ignored:** none - but this state is never forced; it coexists with
  `Monitoring`/`My-*` as an available decision, not a preempting one.

### Outgoing-trade
- **Trigger:** the player begins to assemble an offer.
- **Exit:** the player sends or cancels; a sent offer is resolved
  (`TradeUpdated`).
- **Decisions:** assemble give/receive, send, cancel.
- **Events received:** `TradeUpdated` (accepted/declined), `AuctionOpened`/
  `VoteOpened` (sending is blocked until the window closes), `GameEnded`.
- **Events ignored:** none beyond background.

### Awaiting-verdict
- **Trigger:** the player has offered a bribe; opponents are voting.
- **Exit:** `VoteResolved` - success -> My-move; failure -> the turn ends ->
  Monitoring.
- **Decisions:** none (the player cannot vote on their own bribe).
- **Events received:** `ClockTick`, `VoteResolved`, `ConnectionLost`.
- **Events ignored:** trade events (blocked during the vote).

### Awaiting-others
- **Trigger:** the player has committed a bid or a vote and the window has not
  closed.
- **Exit:** `AuctionResolved`/`VoteResolved` -> the outcome, then prior context.
- **Decisions:** none (the committed choice is locked).
- **Events received:** `ClockTick`, `AuctionResolved`/`VoteResolved`,
  `GameEnded`, `ConnectionLost`.
- **Events ignored:** trade events (blocked).

### Result
- **Trigger:** `GameEnded` or `TimeUp` (a terminal cause). The table stops.
- **Exit:** the player chooses to play again (if they held a seat) or to leave.
- **Decisions:** play again / leave.
- **Events received:** none that reopen play; a new game beginning -> re-enter a
  live CONTEXT.
- **Events ignored:** all game-play events (the game is over).

## MODE overlays

### Seated-active
The default: the player may reach every CONTEXT above as an actor.

### Disconnected
- **Trigger:** `ConnectionLost`.
- **Exit:** `Reconnected` (-> Reorienting); the player leaves.
- **Behaviour:** the game continues; after a grace period `TurnAutoPlayed`
  applies the canonical action to the player's turns; the player's inputs do not
  affect the game. Open windows the player was eligible for resolve without them
  (silent abstain / reject).

### Reorienting
- **Trigger:** `Reconnected`, or joining mid-game.
- **Exit:** orientation complete -> the live CONTEXT.
- **Behaviour:** snap to the true present; never replay missed history; no
  decision is offered until orientation completes.

### Eliminated-watcher
- **Trigger:** the player's bankruptcy or resignation.
- **Exit:** the player leaves; the game ends (-> Result, with play-again if
  still connected).
- **Behaviour:** public view only; the sole action is to leave; no game
  decisions are available.

### Spectator
- **Trigger:** the player entered to watch (never seated).
- **Exit:** the player leaves.
- **Behaviour:** public information only - every in-flight bid and vote is
  masked, no trade offers are shown; the sole action is to leave; no CONTEXT
  offers a decision.

---

# State Transitions

Preemption priority (higher preempts lower): **Result** > **open timed window
(Auction | Vote)** > **the player's own turn (My-move / Jail / My-post-move)** >
**trades (Incoming / Outgoing)** > **Monitoring**. MODE overlays do not preempt;
they change who may act.

```
Monitoring  --TurnPassed(me)-->            My-move            (Jail if imprisoned)
Monitoring  --AuctionOpened-->             Auction
Monitoring  --VoteOpened-->                Vote
Monitoring  --TradeOffered-->              Monitoring (+ Incoming-trade available)
Monitoring  --(I initiate)-->              Outgoing-trade
Monitoring  --GameEnded/TimeUp-->          Result
Monitoring  --ConnectionLost-->            [Disconnected]

My-move     --MovementResolved (unowned landing)--> Auction   (my turn SUSPENDED)
My-move     --MovementResolved (else)-->            My-post-move
My-move     --clock/AFK-->                          auto-play -> My-post-move / turn ends

Jail --held exit-->        My-move
Jail --route chosen-->     My-move
Jail --offer bribe-->      Awaiting-verdict  (opens Vote for opponents)
     Awaiting-verdict --success--> My-move
     Awaiting-verdict --failure--> Monitoring (turn ends)
Jail --clock-->            canonical route committed

My-post-move --develop/take over--> My-post-move (Consequence)
My-post-move --end turn/clock-->    Monitoring

Auction --commit/abstain--> Awaiting-others --AuctionResolved--> prior context
Auction --window closes (no act)--> silent abstain -> prior context
Vote    --accept/reject-->  Awaiting-others --VoteResolved--> prior context
Vote    --window closes (silent)--> reject -> prior context

Incoming-trade --accept/decline/counter--> cleared / Outgoing-trade
Incoming-trade --withdrawn/invalid-->      cleared
Incoming-trade --AuctionOpened/VoteOpened--> SUSPENDED (offer persists)
Outgoing-trade --send--> pending (return to prior context)
Outgoing-trade --cancel--> cleared
Outgoing-trade --AuctionOpened/VoteOpened--> send blocked until window closes

Result --play again--> new game -> Monitoring/My-move
Result --leave-->      surface ends

[Disconnected] --Reconnected--> [Reorienting] --oriented--> live CONTEXT
[Disconnected] --leave-->       surface ends
(any CONTEXT)  --bankrupt/resign--> [Eliminated-watcher]
```

**Interruptions and suspended states.** An opening timed window interrupts
`Monitoring`, deliberation, and trade composition, and blocks trade RESOLUTION;
a suspended `Incoming-trade`/`Outgoing-trade` resumes when the window closes. A
terminal event may fire during an open window (time expires mid-bid; a
development wins the game): `Result` preempts and the window is abandoned. The
player's own turn does NOT preempt an already-open window - the window triggered
by the player's landing resolves before the turn continues. A witnessed
`Consequence` may briefly hold attention but requires no decision and causes no
transition.

---

# Concurrent Activities

**By the player.** Move, manage estate, take a landing option, end the turn;
be pulled into the `Auction` their own landing opens (their turn suspended);
receive a trade offer (available, non-forced); initiate/withdraw trades; end the
game by a development (win by target, or by exhausting a shared resource).

**By other players.** Move; open an `Auction` (by landing) or a `Vote` (by
bribing) that pulls the player in; send the player a trade offer; produce
consequences that involve the player (the player earns rent); end the game.

**By the system.** Advance the shared clocks; fire `TimeUp` (a terminal cause)
at any moment; auto-play the player's turn after the disconnect/AFK grace;
resolve a window when its clock closes; invalidate a stale trade.

**May happen at the same time:** exactly ONE timed window (Auction XOR Vote) for
the whole table; a suspended trade decision alongside it; consequences and
standings updates alongside any state; a terminal event alongside anything (it
wins). **Never at the same time:** two timed windows; any trade resolution and a
timed window; two players' turns; acting while `Spectator` or `Disconnected`; a
game decision and `Result`; acting while `Reorienting`.

---

# Functional Guarantees

- **One primary decision.** At any instant at most one decision is the primary
  one demanded of the player; the preemption priority names it unambiguously.
- **No decision is lost.** A decision the player is eligible to make is always
  made available; when a timed window suspends a trade decision, that decision
  is restored, not discarded, when the window closes; a decision missed under a
  clock resolves to its defined default (abstain / reject / canonical action),
  never to an undefined state.
- **Permanent information is always available.** The permanent set defined in
  `DESIGN/product/INFORMATION_ARCHITECTURE` is available in every state and
  every mode; states change which information becomes primary, never whether the
  permanent set exists.
- **No dependence on interface speed.** The player is never rushed by anything
  except the game's own clocks, which are always available; the surface's
  correctness does not depend on how fast it renders or how quickly the player's
  device reacts; a slow presentation may never shorten a decision window or
  cause a decision to be lost.
- **Truth only.** The surface never presents information the game masks (an
  in-flight bid, another seat's pending vote, a private trade) and never implies
  a state the game did not produce.
- **Continuity across interruption.** Loss of connection, auto-play, and
  elimination change the mode, never end the surface; the game's progress
  remains legible to the player throughout.

---

# Failure Behaviour

- **Network loss.** MODE `Disconnected`: last known state kept and marked stale,
  reconnection status available; the game proceeds; the player's inputs are
  inert; eligible open windows resolve as silent defaults.
- **Reconnection.** MODE `Reorienting`: snap to the true present, one
  orientation, no replay of missed history, then resolve into the live CONTEXT.
- **AFK (connected but not acting).** After the turn/grace period the system
  auto-plays the canonical action (`TurnAutoPlayed`); the fact that a turn was
  auto-played is itself made known to the player (an auto-play is never silent).
- **Expiration (a window or clock closes).** The pending decision resolves to
  its defined default: an unsubmitted bid abstains, an uncast vote rejects, an
  unfinished turn plays the canonical action; the game never waits indefinitely.
- **Game terminated.** `Result`: the table stops, the outcome and final standing
  become available, only next-action decisions remain; a terminal event that
  fires during an open window abandons the window.
- **Spectator.** Public information only, all in-flight bids and votes masked, no
  offers; the sole action is to leave; every otherwise-decision CONTEXT is
  present as information without agency.

---

# Out of Scope

This document never describes, and a reader must never infer from it:

- any interface, control, screen, or navigation affordance;
- any widget, component, or design system;
- any layout, placement, region, geometry, or size;
- any colour, typography, iconography, or visual style;
- any animation, motion, timing curve, or sound;
- any framework or implementation detail;
- any feeling or emotional claim (owned by `DESIGN/PLAYER_EXPERIENCE`);
- any rule definition or balance (owned by the rules and engine documents);
- the cross-surface player behaviour and information model themselves (owned by
  `DESIGN/product/`, referenced and applied here, not restated).

---

# Phase 4 - Self-critique

- **Forgotten states checked.** Added and kept the three distinct "waiting"
  situations the naive word "waiting" hides: `Monitoring` (waiting for my turn),
  `Awaiting-others` (my choice is locked, the window has not closed), and
  `Awaiting-verdict` (I await opponents' vote on my bribe). Also kept the
  connected-but-AFK auto-play situation and the spectator-during-a-window
  situation (a window occurs but its contents are masked and no decision is
  offered). No further orphan state found: every `(MODE, CONTEXT)` a player can
  occupy is covered or explicitly impossible.
- **Impossible transitions confirmed.** `Auction -> Vote` directly (each returns
  to the prior context first); `Spectator -> My-move` (no seat); `Result -> any
  play decision`; `Reorienting -> acting` (orientation completes first). These
  are stated, not left implicit.
- **Ambiguities resolved.** Expropriation/takeover is a DECISION within
  `My-post-move`, not a top-level state. `Incoming-trade` is an available, never
  forced, decision - it coexists with `Monitoring`/`My-*` rather than preempting
  them. "One primary decision" is defined by the preemption priority, so
  "primary" is never ambiguous.
- **Boundary honesty (SPECS vs DESIGN).** "Permanent information" and "which
  information becomes primary" are REFERENCED from `DESIGN/product/`, not
  re-derived; the document owns the state machine (states, transitions,
  concurrency, guarantees) which `product/` does not contain. The one phrase that
  recurs across `product/`, `SCREEN_ARCHITECTURE`, and here - "one primary
  decision" - is inherited from the common parent `product/`; it is applied here
  as a guarantee, not redefined. This is the sharpest boundary and the one to
  watch: if a future edit starts describing WHICH information or WHERE, it has
  left SPECS.
- **Convertibility test applied.** Every statement was checked to be honourable
  equally by a paper wireframe and a live interface: none names a control, a
  position, a colour, or a motion; the guarantees are behavioural (availability,
  preemption, defaults, truth), not presentational.
- **Residual risk.** The document is RULES-coupled: if a mechanic changes (a new
  phase, a change to how a window resolves), the affected states/transitions must
  change with it. This is expected for a SPECS document (its lifetime tracks the
  rules, not the interface), and it is the reason this specification is a strong
  candidate to become executable - the state machine and its "never coexist"
  matrix can be checked against the engine's actual phases rather than trusted as
  prose.
