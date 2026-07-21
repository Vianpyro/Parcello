# LobbyScreen - functional specification

A SPECS document (see `SPECS/README.md`): observable functional behaviour only.
It describes WHAT the lobby does, never how it looks. It applies the
cross-surface model owned by `DESIGN/product/`; it does not restate it. Every
statement must be honourable equally by a paper wireframe and by a live
interface.

The lobby is the surface on which a room is ASSEMBLED before play. It exists
only while the room is in its pre-game phase; when the room becomes a live game,
this surface ends and the GameScreen (`GAME_SCREEN.md`) begins.

Two orthogonal dimensions describe every situation:

- **MODE** (the participant's standing in the room): `Host`, `Member`,
  `Observer`, `Disconnected`, `Reorienting`.
- **CONTEXT** (what the room is doing): `Assembling`, `Launching`.

A situation is a `(MODE, CONTEXT)` pair. Unlike the game, the lobby has no timed
windows and no off-turn decisions; its behaviour is dominated by roster changes
and the single host-gated hand-off to play.

Two facts from the mechanics shape everything below and are stated once here:

- **The host is whoever occupies the first seat.** There is no separate host
  identity; host power always follows seat position zero.
- **In the lobby, removing a participant COMPACTS the roster.** A leave or a
  disconnection frees the seat entirely and the remaining seats close the gap,
  so seat positions after the departed one shift by one. Anything that is a
  function of seat position - the host role, and each seat's derived identity -
  shifts with it. (This is unlike a live game, where a seat is held on
  disconnection and positions never move.)

---

# Purpose

The lobby exists so that a group can be ASSEMBLED into a valid, agreed starting
configuration before a game begins: the right participants are present, the
house rules are settled and visible to all, and exactly one participant - the
host - holds the authority to launch. Its promise: everyone sees the same
roster and the same rules at all times; no one but the host can change the
room; and the room can only start when it is genuinely ready.

---

# Lifecycle

**Appears** when a participant creates a room (they become its host) or joins an
existing room that is still assembling.

**Disappears** when: the participant leaves (returning to a prior surface, the
connection remaining reusable to create or join again); the room is launched
into a live game (the surface hands off to the GameScreen); or the room
dissolves under them.

**Reactions:**

- **Reconnection.** Because a lobby disconnection frees the seat (see below),
  there is no held seat to restore: reconnecting to the lobby is a fresh JOIN,
  subject to availability - it succeeds if the room still exists and has room,
  and is refused otherwise. (Held-seat reconnection exists only once the room is
  a live game.) A successful re-join enters `Reorienting`, then `Assembling`.
- **Disconnection.** In the lobby, a disconnection FREES the participant's seat
  immediately - functionally identical to leaving. The roster compacts;
  positions, the derived identities, and the host role may shift for the
  remaining participants.
- **Abandon (leave).** The participant's seat is freed (roster compacts); the
  surface ends for them.
- **Elimination.** Not applicable in the lobby - no one is eliminated before
  play begins.

---

# Functional States

Functional events the surface receives (named neutrally): `SeatJoined`,
`SeatLeft`, `BotAdded`, `BotRemoved`, `SettingsChanged`, `HostChanged` (implied
by a compaction that moves seat zero), `ObserverJoined`/`ObserverLeft`,
`GameStarted`, `RoomDissolved`, `FunctionalError`, `ConnectionLost`,
`Reconnected`. Across all states, `SeatJoined`/`SeatLeft`/`BotAdded`/
`BotRemoved`/`SettingsChanged` keep the roster and the rules current for
everyone; "events ignored" below means "does not create or change a decision
in this state", never "the roster stops updating".

## CONTEXT: Assembling

### Assembling - Host
- **Trigger:** the participant occupies the first seat - by creating the room,
  or by a compaction that promoted them to seat zero.
- **Exit:** the participant leaves; a compaction demotes them from seat zero
  (-> Member); the room launches (-> Launching); the room dissolves.
- **Decisions:** add a bot (while the room is not full); remove a bot; change
  the room's rules and clocks; start the game (only when at least two seats are
  filled); leave; share the room's code.
- **Events received:** `SeatJoined`, `SeatLeft` (may demote via compaction),
  `SettingsChanged` (echo of the host's own accepted change), `GameStarted`
  (-> hand-off), `RoomDissolved`, `FunctionalError`, `ConnectionLost`.
- **Events ignored:** `ObserverJoined`/`ObserverLeft` do not change any host
  decision (observers do not affect readiness or launch).

### Assembling - Member
- **Trigger:** the participant occupies a seat other than the first.
- **Exit:** the participant leaves; a compaction promotes them to seat zero
  (-> Host); the room launches (-> hand-off); the room dissolves.
- **Decisions:** leave. (A member sees the roster and the rules but cannot
  change the room or start it.)
- **Events received:** `SeatJoined`, `SeatLeft`, `BotAdded`, `BotRemoved`,
  `SettingsChanged`, `HostChanged`, `GameStarted`, `RoomDissolved`,
  `ConnectionLost`.
- **Events ignored:** none beyond the background updates.

### Assembling - Observer
- **Trigger:** the participant attaches to the room as a watcher without taking
  a seat. A seated participant cannot observe their own room.
- **Exit:** the participant leaves; the room launches (the observer then follows
  the game as a spectator on the game surface); the room dissolves.
- **Decisions:** leave only.
- **Events received:** roster and rules updates (informational), `GameStarted`
  (-> becomes a spectator of the live game), `RoomDissolved`.
- **Events ignored:** everything actionable - an observer has no seat, no host
  power, and no readiness role; while the room is still assembling there is no
  game to observe, only the roster and the rules.

## CONTEXT: Launching

- **Trigger:** the host requests a start while the room is valid (host, at least
  two seats).
- **Exit:** the room's rules are accepted -> the room becomes a live game (the
  surface hands off to the GameScreen for every seated participant, and
  observers become spectators); the rules are rejected as invalid -> back to
  `Assembling` with a functional error, the room unchanged.
- **Decisions:** none (the launch is in flight).
- **Events received:** `GameStarted` (-> hand-off) or `FunctionalError` (invalid
  rules -> Assembling).
- **Events ignored:** further roster changes do not alter an accepted launch.

## MODE overlays

### Disconnected
- **Trigger:** `ConnectionLost` while in the lobby.
- **Behaviour:** the seat is freed (the lobby holds no seat for a disconnected
  participant); the roster compacts for the others. There is no "waiting to
  reconnect into the same seat" in the lobby.

### Reorienting
- **Trigger:** a fresh join/re-join into an assembling room.
- **Exit:** orientation complete -> `Assembling` in the appropriate role.
- **Behaviour:** present the current roster and rules as they truly are; no
  history replay.

---

# State Transitions

The lobby has no preemption between decisions (no timed windows). Its
transitions are roster- and authority-driven:

```
(none)      --create room-->                Assembling-Host   (seat zero; a room code exists)
(none)      --join by code (space free)-->  Assembling-Member (or Host if it becomes seat zero)
(none)      --join a bot-full room-->       the newest bot is evicted, then Assembling-Member
(none)      --join a human-full room-->     FunctionalError (refused), surface not entered
(none)      --join invalid/absent code-->   FunctionalError (refused)

Assembling-Host --add bot (not full)-->     Assembling-Host (roster grows by a bot)
Assembling-Host --remove bot-->             Assembling-Host (newest bot removed)
Assembling-Host --change rules-->           Assembling-Host (rules broadcast to all)
Assembling-Host --start (>=2 seats)-->      Launching
Assembling-*    --leave-->                  surface ends; roster compacts for the rest
Assembling-*    --ConnectionLost-->         seat freed; roster compacts (same as leave)

SeatLeft(seat 0 occupant) --compaction-->   HostChanged: the next seat becomes Host
SeatLeft / SeatJoined      --compaction-->   remaining seats' positions (and derived identities) shift

Launching --rules accepted-->               GameStarted -> hand-off to GameScreen (surface ends)
Launching --rules rejected-->               FunctionalError -> Assembling-Host (room unchanged)

Assembling-* --last seat and observer gone--> RoomDissolved (immediate) -> surface ends
Assembling-* --room idle with no connected participant--> RoomDissolved (after the idle limit)

Observer --attach--> Assembling-Observer
Observer --leave-->  surface ends
Observer --GameStarted--> becomes a spectator on the game surface
```

**Interruptions and suspended states.** There are none in the classic sense: the
lobby carries no timed decision that another can preempt. The only "interrupt"
is structural - a compaction (from a leave or disconnection) that reassigns the
host role and shifts positions; it changes authority and identity, not a
decision in flight. A launch, once accepted, is terminal for the surface.

---

# Concurrent Activities

**By the participant.** Create; join; leave; (as host) add/remove a bot, change
the rules, start; (as observer) attach and leave; share the code.

**By other participants.** Join or leave (each changing the roster and possibly
the host and positions); (the host) add/remove bots, change the rules, start the
game; observers attach or leave.

**By the system.** Evict the newest bot when a human joins a full room; broadcast
every roster and rules change to all participants; dissolve the room when it is
empty, or after the idle limit with no connected participant; issue each
seat the credential it will later need to reclaim its seat once the game is
live.

**May happen at the same time:** roster changes and rules changes from the host,
observed by everyone consistently; observers attaching/leaving alongside seat
changes. **Never at the same time:** two participants occupying the same seat; a
host power exercised by a non-host; a launch and a rules change (a launch is
gated on the current, accepted rules); the surface being both a lobby and a live
game (the hand-off is a single, one-way transition).

---

# Functional Guarantees

- **Single authority.** Exactly one participant - the occupant of the first seat
  - holds host power at any instant; a non-host can never mutate the room
  (add/remove a bot, change the rules, or start). Host power is never held by
  two participants and, while any seat remains, never by none.
- **Deterministic host continuity.** The host role follows seat position zero;
  when the host leaves, the role migrates deterministically to the next seat
  with no election and no host-less interval (unless the room is now empty, in
  which case it dissolves).
- **Consistent shared truth.** Every participant sees the same roster and the
  same rules; every roster or rules change is reflected to all before the next
  action depends on it.
- **No silent seat loss.** A seat only leaves the roster by an explicit leave or
  a disconnection, and either is reflected to everyone.
- **Gated, all-or-nothing launch.** A game starts only when the host requests it,
  at least two seats are filled, and the rules are valid; a rejected launch
  leaves the room exactly as it was, never half-started.
- **Deterministic identity.** Each seat's derived identity is a pure function of
  its position; every participant computes the same mapping, and it changes only
  when positions change (a lobby compaction), never by choice.
- **Deterministic dissolution.** The room persists while any seat or observer
  remains and dissolves deterministically when none do (immediately when the
  last leaves, or after the idle limit when only disconnected participants
  remain).
- **No dependence on interface speed.** Nothing in the lobby is timed against the
  participant; correctness never depends on how fast the surface reacts.

---

# Failure Behaviour

- **Network loss.** The seat is freed and the roster compacts (equivalent to
  leaving); there is no held-seat wait in the lobby.
- **Reconnection.** A fresh join subject to availability: it succeeds if the room
  still exists with space, and is refused otherwise; on success, `Reorienting`
  then `Assembling`.
- **AFK.** There is no per-participant clock in the lobby; idling is harmless. A
  room with no connected participant dissolves only after the idle limit.
- **Expiration.** The only time limit is the room-idle dissolution above; there
  is no decision that expires in the lobby.
- **Room terminated.** `RoomDissolved` ends the surface; the participant is
  returned to a prior surface with the connection reusable.
- **Observer.** Sees only the roster and rules while the room assembles (no game
  exists to observe); the sole action is to leave; on launch, the observer
  continues as a spectator of the live game.

## Functional errors (all refused without side effect)

- Joining with an absent or malformed room code.
- Joining a room already full of human participants.
- A non-host attempting any host power (add/remove bot, change rules, start).
- Starting with fewer than two seats.
- Starting with rules the room rejects as invalid (stays assembling, with the
  reason).
- Adding a bot when the room is already full, or outside the assembling phase.
- Re-claiming a spoofable seat without the credential that seat requires.

Each error is a no-op on the room's state and is reported with its reason; none
leaves the room in a partial state.

---

# Out of Scope

This document never describes, and a reader must never infer from it:

- any interface, control, or navigation affordance;
- any widget, component, or design system;
- any layout, placement, region, geometry, or size;
- any colour VALUE, typography, iconography, or visual style (a seat's derived
  identity is named as a function of position, never as a specific colour);
- any animation, motion, timing curve, or sound;
- any framework or implementation detail;
- any feeling or emotional claim (owned by `DESIGN/PLAYER_EXPERIENCE`);
- any rule definition or balance (owned by the rules and engine documents);
- the cross-surface model itself (owned by `DESIGN/product/`).

---

# Phase 4 - Self-critique

- **Two requested topics do not exist as mechanics; specified truthfully, not
  invented.** "Colour choice" is NOT a decision in Parcello: a seat's identity
  (its colour) is a pure function of seat position, and it CHANGES only when a
  lobby compaction shifts positions - never by a player picking. "Ready status"
  is NOT a mechanic either: there is no per-seat ready flag; readiness is
  implicit (a filled seat will play) and the launch gate is host-initiated with
  at least two seats. Both topics are covered by the real behaviour ("derived
  identity", "gated launch") and are flagged here because SPECS describes
  observable behaviour, not desired features. If a colour-picker or a ready-check
  is ever wanted, that is a rules/product change - and only then a spec.
- **The strongest, easily-missed truth: the lobby frees seats, the game holds
  them.** In the lobby a disconnection removes the seat and compacts the roster
  (shifting positions, derived identities, and the host); in a live game the seat
  is held for reconnection and positions never move. This asymmetry drives the
  reconnection behaviour (lobby re-join is a fresh join, not a seat restore) and
  is stated up front so it cannot be assumed away.
- **Host is positional, migration is deterministic.** Host = occupant of seat
  zero; a leave that empties seat zero promotes the next seat with no election.
  No host-less lobby exists except an empty (dissolving) one. Confirmed against
  the mechanics, not assumed.
- **Observer boundary honest.** An observer may attach while the room assembles
  but has no game to observe (only the roster and rules); at launch they continue
  as a spectator on the game surface. The lobby is otherwise seats-only.
- **Forgotten states checked.** `Launching` is kept distinct from `Assembling`
  because a launch can FAIL (invalid rules) and return; the failure path is a
  real state edge, not an error footnote. The bot-eviction-on-join and the
  human-full refusal are distinct join outcomes, both covered.
- **Convertibility test applied.** No statement names a control, a position, a
  colour value, or a motion; identity is named as "a function of seat position",
  never as a hue; every guarantee is behavioural (authority, consistency,
  determinism, all-or-nothing launch).
- **Residual risk.** Like the GameScreen spec, this is rules-coupled: the
  host-is-seat-zero rule, the compaction-frees-seat rule, the launch gate, and
  the dissolution rule are engine behaviours; if any changes, the affected states
  and guarantees change with it. That coupling is expected and makes this
  specification checkable against the room state machine rather than trusted as
  prose.
