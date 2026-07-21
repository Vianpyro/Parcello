# Functional Events - specification

A SPECS document (see `SPECS/README.md`): observable functional behaviour only.
It specifies the functional events of Parcello - what can happen, who can cause
it, what it causes, and the cross-cutting rules of ordering, visibility,
persistence, and incompatibility. It describes no interface, no wire format, and
no implementation; the visual response to an event is owned by the motion
documents, and the concrete types by the engine.

An "event" here is a functional FACT that something happened. Two families:

- **Game events** - facts the game engine produces when it accepts a player
  command or advances its own state. They are deterministic and replayable.
- **Session events** - facts the room/session layer produces (seats, connection,
  matchmaking, history). They are not part of the game replay.

Each event below is fully specified by its own fields (origin, actor,
preconditions, consequences, secondary events, incompatible-with) PLUS the four
cross-cutting fields (interruption, visibility, persistence, guaranteed order),
which take the GLOBAL DEFAULTS defined next unless an event notes an override.

---

# Global properties (the defaults every event inherits)

- **Actor.** An event's actor is either a PLAYER (it results from a command that
  player issued) or the SYSTEM (the engine or the server produced it without a
  command). Bots are players for this purpose.
- **Origin.** Every game event originates either from an accepted command or as a
  secondary consequence of another event (a chain). No game event originates from
  the interface; the interface only issues commands.
- **Guaranteed order & determinism (default).** Within one game, events occur in
  a fixed order: the emission order of a single accepted command is fixed, and
  commands take effect in the order they are accepted. Given the initial
  participants, the seed, and the ordered list of accepted commands, the entire
  event stream reproduces identically. All randomness comes from one seeded
  source; there is no other source of order or chance.
- **Interruption & atomicity (default).** A command that is rejected produces NO
  event and changes nothing (a rejection is reported only to its issuer). Once an
  event is emitted, it is final - it cannot be un-happened, and its state change
  is atomic. What CAN be cut short is the WITNESSING of an event (a long chain may
  be compressed), never the fact or the state.
- **Visibility (default).** An event is visible to every participant, through a
  per-seat view. The seeded randomness source and any undrawn order are NEVER in
  any view. A spectator sees only public facts. Overrides below name the events
  that are masked or private.
- **Persistence (default).** A game event is reproducible from the persisted,
  ordered log of accepted commands plus the seed; the game's outcome and the
  post-game survey are stored. Individual ephemeral events are not stored as
  such - they are re-derivable. Session events are NOT part of the replay and are
  not reproducible from it.

## Global incompatibility principles

- **Auction solvency.** While a sealed-bid auction is open, no cash may change:
  the four trade events and every player-initiated cash-moving event are refused,
  so they cannot occur. The only player event during an open auction is a bid
  submission.
- **One timed window.** A sealed-bid auction and a bribe vote never coexist; at
  most one timed window is open at a time.
- **Terminal finality.** A terminal event (a win, or time expiring) is the last
  event of the game; no game event follows it.
- **One actor at a time.** The events of two players' turns never interleave;
  simultaneous windows (auction, vote) are the only multi-actor events, and they
  collect one contribution per living participant.

---

# Game events

Format per event: origin - actor - preconditions - consequences - secondary
events - incompatible-with. Cross-cutting fields appear only as overrides.

## Turn and movement

- **TurnStarted** - engine advances to the next living seat - SYSTEM - the prior
  turn ended - the acting seat changes; an emptied hand refills (advancing the
  round metronome) - secondary: a hand refill may complete a round and trigger
  `RoundBonusAwarded` - incompatible: never inside another turn.
- **MovementCardPlayed** - PlayMovementCard command - the acting player - it is
  the player's turn, in the move phase, and the value is available (in hand, or
  the front card of a committed jail route) - the value is consumed - secondary:
  `Moved` (and everything the landing causes).
- **Moved** - `MovementCardPlayed`, a card teleport, or a jail transition - the
  moving player (or SYSTEM for a teleport) - a movement occurred - position
  changes; crossing the start pays salary - secondary: `SalaryPaid` on crossing
  start; then the landing's events (`BlindAuctionOpened`, `RentPaid`, `TaxPaid`,
  `CardDrawn`, `SpotlightStarted`, `WentToJail`, ...).
- **SalaryPaid** - crossing the start tile - SYSTEM - a move crossed start
  forward - the mover gains salary.
- **CardDrawn** - landing on a draw tile - SYSTEM - the lander hit a chance tile
  - a card's effect applies (a chain, bounded in depth) - secondary: the card's
  effect events (cash, movement, jail...). Visibility override: the drawn card's
  EFFECT is public; the deck's remaining order is never in any view.

## Money

- **RentPaid** - landing on an owned, unmortgaged tile - the lander (payer) -
  the tile is owned by someone else and charges rent - cash moves from payer to
  owner - incompatible: not while an auction is open.
- **TaxPaid** - landing on a tax tile - the lander - a tax is due - cash leaves
  the lander to the bank - secondary: a tile threat marker.
- **CashAdjusted** - a card, an audit, or another rule effect - SYSTEM (or the
  affected player via a command that moves cash) - a rule changed a player's
  cash by a delta - the player's cash changes - incompatible: not while an
  auction is open (player-initiated).
- **HouseBuilt** - Build command - the acting owner - the player's turn, the
  building rule allows it, the shared pool can supply it, even-build holds - a
  development level is added, cash paid, the shared pool decremented - secondary:
  `WonByPoolExhaustion` if the pool empties; `WonByPoints` if it crosses the
  target.
- **HouseSold** - SellHouse command, or forced liquidation - the owner or SYSTEM
  - even-sell holds - a development level is removed, cash refunded, the pool
  restored - note: forced liquidation may coalesce a whole estate into one fact.
- **PropertyMortgaged** - Mortgage command - the owner - the group is house-free
  - the tile is mortgaged, cash received.
- **PropertyUnmortgaged** - Unmortgage command - the owner - the redemption cost
  is affordable - the tile is redeemed, cash paid.
- **RentBoosted** - BoostRent command - the owner - the boost rule is on, the
  tile is eligible - a one-shot rent boost is armed, cash paid.
- **RentBoostConsumed** - the first rent collected at a boosted tile - SYSTEM -
  an armed boost existed - the boost is spent (one-shot) - secondary: it inflates
  the concurrent `RentPaid`.

## Sealed-bid auction

- **BlindAuctionOpened** - landing on an unowned property - SYSTEM - the tile is
  unowned - a timed window opens for the whole table - incompatible: opens no
  vote; blocks trades and cash moves while open (auction solvency).
- **BlindBidSubmitted** - SubmitBlindBid command - any living seat - the window
  is open, the seat has not yet submitted, and the amount is a valid commitment
  (at least the floor and at most the seat's cash, or zero to abstain) - the
  seat's commitment is recorded - interruption: if the window closes first, the
  seat auto-abstains (a default, not this event). Visibility override: the AMOUNT
  is masked to others until resolution; only the fact of having committed may be
  public.
- **BlindAuctionResolved** - the window closing - SYSTEM - the window elapsed or
  all living seats committed - the winner is determined; every commitment becomes
  public at once; the winner pays in full - secondary: `PropertyTransferred`
  (band to the winner), `DiscovererRefunded` (if the discoverer won), cash
  movements. Visibility override: the moment all commitments become public.
- **DiscovererRefunded** - a discoverer winning their own tile - SYSTEM - the
  winner was the discoverer - the bank refunds a fixed share to the discoverer.

## Ownership and aggression

- **PropertyTransferred** - an auction win, an expropriation, an accepted trade,
  or a bankruptcy release - the acquirer or SYSTEM - ownership changed - the
  tile's owner (or "unowned", on a bankruptcy release) changes; a whole portfolio
  may transfer as one fact.
- **Expropriated** - Expropriate command - the acting player - end of the
  player's turn, on a landed rival tile, with the expropriation rule on -
  ownership transfers at a premium; an improved tile liquidates, its former owner
  compensated - secondary: `PropertyTransferred`, cash movements, a tile threat.

## Jail and corruption

- **WentToJail** - a go-to-jail tile or card - the jailed player (or SYSTEM) -
  the player hit a jail trigger - the player is imprisoned (a teleport, no path).
- **JailCardReceived** - a card grant - the receiving player - a card effect
  granted it - the player's jail-card count rises (a count, never an object).
- **JailCardUsed** - UseJailCard command - the jailed player - the player is
  jailed and holds a card - immediate release - secondary: `LeftJail`, then a
  normal move.
- **LegalRouteChosen** - ChooseLegalRoute command - the jailed player - the
  player is jailed and offers a valid permutation of a fresh hand - the route is
  locked, the hand replaced, the first card plays - secondary: `LeftJail`,
  `Moved`; the route holder's tiles charge no rent while the route lasts.
- **LeftJail** - a successful exit (card, route, or accepted bribe) - the exiting
  player or SYSTEM - an exit resolved - the player is free.
- **BribeOffered** - OfferBribe command - the jailed player - the player is
  jailed and offers an amount within their cash - a bribe vote window opens for
  living opponents - secondary: opens the vote - incompatible: opens no auction;
  blocks trades while the vote is open.
- **BribeVoteCast** - VoteOnBribe command - a living opponent (not the briber) -
  the vote window is open, the voter has not cast - the vote is recorded -
  interruption: a silent voter auto-rejects at close. Visibility override: each
  cast is masked until resolution (pending votes are hidden).
- **BribeResolved** - the vote window closing - SYSTEM - the window elapsed or all
  eligible voted - the outcome is public; on success the amount splits among the
  opponents and the briber exits; on failure no cash moves and the turn ends -
  secondary: `LeftJail` and cash movements on success. Visibility override: the
  casts become public at resolution.

## World / economy

- **MarketEventActivated** - the forecast queue reaching a scheduled event -
  SYSTEM - a scheduled market event became active - a public multiplier applies -
  note: the schedule (which events are coming) is public; the generator is never
  in any view.
- **MarketEventExpired** - a market event's duration ending - SYSTEM - the active
  event elapsed - its multiplier lifts.
- **SpotlightStarted** - landing on the exposition tile - SYSTEM - the lander hit
  the exposition corner - a random tile enters the spotlight (bonus rent), the
  draw made from the seeded source - note: the spotlit tile is public; the draw
  source is not.
- **SpotlightEnded** - the spotlight's duration ending or a re-roll - SYSTEM - the
  spotlight elapsed or a new landing replaced it - the prior spotlight lifts.
- **RoundBonusAwarded** - the last surviving player completing a hand refill -
  SYSTEM - a full round completed - a fixed, non-reversible victory-point bonus
  banks to whoever is strictly richest at that instant - secondary of the refill
  in `TurnStarted`.

## Trade

- **TradeProposed / TradeAccepted / TradeDeclined / TradeCancelled** - the four
  trade commands - a solvent player (proposer or recipient) - it is not during an
  auction or a vote; an acceptance is re-validated (a stale offer is refused with
  no event); a proposer holds at most a small number of open offers - proposed:
  an offer exists; accepted: the two estates/cash swap (`PropertyTransferred`,
  cash); declined/cancelled: the offer clears - incompatible: none of the four
  can occur while an auction or a vote is open. Visibility override: all four are
  PRIVATE to the two parties; no third participant, and no spectator, sees an
  offer or its lifecycle. Persistence: an accepted trade changes state and
  replays; the private lifecycle events are not surfaced to others.

## Terminal

- **PlayerBankrupt** - a debt a player cannot fully pay - the bankrupt player or
  SYSTEM - the player's liquid worth cannot cover a due amount - the player is
  out; their estate is released to the bank (unowned), the creditor takes only
  the residual cash - secondary: `PropertyTransferred` (estate release),
  possibly `GameEnded` (last standing).
- **PlayerResigned** - Resign command - the resigning player - the player chose
  to quit - same as bankruptcy: out, estate released - secondary: as above.
- **GameEnded** - the last player standing, or the game concluding - SYSTEM - one
  seat remains, or a conclusion condition fired - the game stops with a winner -
  incompatible: nothing follows (terminal).
- **WonByPoints** - a player reaching the victory-point target - SYSTEM - the
  target was crossed - immediate win - terminal.
- **WonByGroups** - a player completing the required full groups (when that
  optional condition is on) - SYSTEM - the domination condition met - win -
  terminal.
- **WonByPoolExhaustion** - a build emptying the conglomerate pool - SYSTEM - the
  pool hit zero - the game ends immediately, highest score wins - terminal.
- **TimeUp** - the game clock expiring - SYSTEM - the timed game reached its
  limit - richest by net worth wins - terminal.

---

# Session / room events (not part of the game replay)

Actor and origin as noted; visibility is the room's participants unless masked;
none is reproducible from the game replay; ordering is the serial order of the
room's processing.

- **SeatJoined / SeatLeft** - a participant joining/leaving, or a disconnection -
  a participant or SYSTEM - roster changes; in the lobby a leave compacts the
  roster (positions, derived identities, and host may shift); in a live game a
  seat is held for reconnection.
- **BotAdded / BotRemoved** - the host, in the lobby - the host - a bot seat is
  added (up to capacity) or the newest removed; a human joining a full-of-bots
  room evicts the newest bot.
- **SettingsChanged** - the host, while assembling - the host - the shared
  configuration is replaced (coerced to bounds) and reflected to all.
- **GameStarted** - a host start or an automatic match start - the host or SYSTEM
  - the room becomes a live game; the settings freeze into it.
- **RoomDissolved** - the room emptying, or an idle limit - SYSTEM - no seat or
  observer remains, or the room idled - the room ceases to exist.
- **ConnectionLost / Reconnected** - a transport change - SYSTEM - a participant's
  link dropped or returned; last connection for an identity wins.
- **TurnAutoPlayed** - an AFK or disconnected acting seat past its grace - SYSTEM
  - the canonical action was applied on the seat's behalf; never silent to the
  seat.
- **RatingsUpdated** - a ranked game ending - SYSTEM - each participant's rating
  change is produced (ranked only).
- **FeedbackRecorded** - a post-game survey submission - a seated participant,
  once, in the finished phase - the sanitized rating/comment is stored.
- **SpectatorJoined / SpectatorLeft** - a watcher attaching/detaching - a watcher
  or SYSTEM - the watcher set changes (capped); a watcher sees only public facts.

---

# Out of Scope

This document never describes, and a reader must never infer from it:

- any interface, control, layout, colour, typography, animation, or sound (the
  visual/audio response to an event is owned by the motion and audio documents);
- any wire format, serialization, type, or field name (owned by the protocol and
  engine);
- any framework or implementation detail;
- rule DEFINITIONS or numeric values (owned by the rules and engine documents;
  this document names events and their functional relations, not the formulae);
- any feeling or emotional claim (owned by `DESIGN/PLAYER_EXPERIENCE`).

---

# Phase 4 - Self-critique

- **This layer merits existing, and it was un-owned.** The functional
  event catalogue - who can cause each event, what it chains to, and above all
  the cross-cutting rules of order, visibility/masking, persistence, and
  incompatibility - lived nowhere as a whole: the engine owns the TYPES, the
  motion documents own the visual BEATS of the subset that animates, and the
  view/history rules are scattered across the architecture. This document owns
  the functional semantics. Created as `SPECS/EVENTS.md`.
- **Completeness checked against the source.** The game-event list was taken from
  the engine's actual event enumeration (44 facts), not from memory, so events
  that never animate (turn start, bid submission, jail-card receipt, vote cast,
  market expiry, spotlight end, resignation) are included, not just the ones with
  a visible beat. Session events are listed separately because they do not
  replay.
- **The four cross-cutting fields are stated once and inherited.** Repeating
  "order guaranteed by the accepted-command log", "rng never visible", "rejections
  never mutate", and "reproducible from the log" on 44 events would be noise; they
  are global defaults, and each event states only its overrides (masked bids,
  masked votes, private trades, the public-schedule/hidden-generator split). This
  keeps the spec complete without 400 repeated lines.
- **The load-bearing incompatibilities are explicit, not implied.** Auction
  solvency (no cash or trade event while a bid window is open), the one-timed-
  window rule (auction XOR vote), terminal finality, and one-actor-at-a-time are
  stated as principles and referenced per event, because they are the constraints
  a reimplementation is most likely to get wrong.
- **Boundary honesty.** Visibility here is functional (who may KNOW a fact), never
  presentational (how it is shown); persistence is functional (reproducibility
  from the log), never storage layout. The overlap with the motion documents is
  deliberate and one-directional: they own the BEAT, this owns the FACT; neither
  restates the other.
- **Residual risk.** This is the most rules-coupled SPECS document: every event's
  preconditions and secondary chains are engine behaviour. If the engine adds an
  event or changes a chain, this catalogue must change with it - which, like the
  determinism it rests on, makes it directly checkable against the engine's event
  stream rather than trusted as prose.
- **Convertibility test applied.** Every statement is a fact, a condition, a
  consequence, or a relation between events; none names a control, a position, a
  colour, or a motion. A paper model of the event flow and a running game would
  agree on all of it.
