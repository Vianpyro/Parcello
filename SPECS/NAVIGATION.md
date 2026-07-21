# Navigation - functional specification

A SPECS document (see `SPECS/README.md`): observable functional behaviour only.
It specifies WHAT the inter-surface navigation of Parcello does - which surfaces
exist, when the player moves between them, which moves are impossible, and how
interruptions and reconnection reroute the player. It describes no interface,
control, or layout; the per-surface behaviour of the game, lobby, and settings
lives in their own SPECS documents, and this document owns only the state
machine BETWEEN surfaces.

A "surface" here is a navigation state, not a visual screen. `Result` is a
terminal STATE of the `Game` surface (see `game/GAME_SCREEN.md`), not a separate
surface; it is treated as such below.

---

# Surfaces and responsibilities

- **Connect** (the root). Establish a server and an identity (guest, or a
  signed-in token). Responsibility: reach a usable connection; nothing past it
  is reachable without it.
- **Sign-in** (a sub-flow reached from Connect). Obtain an identity token
  through an external authority, then return to Connect carrying it.
- **Menu**. Choose a path: create a private room, join one by code, watch a
  game, enter ranked (when available), read the rules, or manage a local server
  / discover LAN servers (where those exist). Responsibility: the single hub
  from which every play path departs.
- **Rules** (a leaf reached from Menu). Present the game's reference; return to
  Menu. Responsibility: reference only - it never starts or joins a game.
- **Server-manager** and **LAN-discovery** (leaves reached from Menu where they
  exist). Manage or find a server, then feed a connection back to Connect/Menu.
  Responsibility: local infrastructure only.
- **Lobby** (a private room being assembled). Assemble a room until it launches;
  see `game/LOBBY_SCREEN.md`. Responsibility: reach a valid, agreed start.
- **Ranked-queue**. Wait to be matched; on a match, hand off directly into a
  game. Responsibility: matchmaking wait with a live signal.
- **Game** (a live game). Participate in, or watch, a live game until it ends and
  the player leaves the result; see `game/GAME_SCREEN.md`. It has two entry
  modes - seated and spectator - and a terminal state `Result`. Responsibility:
  the play surface.

---

# Entry and exit conditions

| Surface | Entered when | Left when |
|---|---|---|
| Connect | the application starts; or a connection is lost back to the root | a connection and identity are established (-> Menu) |
| Sign-in | requested from Connect | a token is obtained or the attempt is abandoned (-> Connect) |
| Menu | a connection and identity exist | a path is chosen; or the player disconnects (-> Connect) |
| Rules | requested from Menu | dismissed (-> Menu) |
| Server-manager / LAN-discovery | requested from Menu (where they exist) | dismissed, or a server is chosen (-> Menu/Connect) |
| Lobby | a private room is created or joined | the room launches (-> Game); the player leaves (-> Menu); the room dissolves (-> Menu) |
| Ranked-queue | ranked is available AND the player holds a token, and they enter the queue | a match is found (-> Game); the player cancels; the queue aborts (-> Menu) |
| Game | a room launches, a player rejoins a live game, or the player enters to watch | the player leaves from the `Result` state or as a watcher (-> Menu); play-again restarts the same game (-> Game); ranked replay returns to the queue (-> Ranked-queue) |

Global entry rule: no surface past Connect is reachable without a connection and
identity; the game is never reachable without first assembling or joining a
room, or a match being made, or an active game existing to watch.

---

# All transitions

```
(start) ----------------------------> Connect
Connect --request sign-in-----------> Sign-in --token / abandon--> Connect
Connect --connection established----> Menu
Connect --probe: server unreachable-> Connect (stays; liveness signalled, never navigates)
Connect --probe: guests refused-----> Connect (guest path disabled; sign-in required)

Menu --create private (default)-----> Lobby (as host)
Menu --create modded----------------> Lobby (as host)
Menu --join by code (valid, room)---> Lobby (as member)
Menu --join by code (invalid/full)--> Menu (refused with reason; no navigation)
Menu --watch (a game exists)--------> Game (spectator mode)
Menu --watch (none)-----------------> Game (spectator mode, the always-available showcase)
Menu --ranked (available + token)---> Ranked-queue
Menu --rules------------------------> Rules --dismiss--> Menu
Menu --manage/discover server-------> Server-manager / LAN-discovery --> Menu/Connect
Menu --disconnect-------------------> Connect

Lobby --host starts (>=2, valid)----> Game (seated; all seats hand off together)
Lobby --host starts (invalid rules)-> Lobby (launch refused with reason)
Lobby --leave-----------------------> Menu
Lobby --room dissolves--------------> Menu
Lobby --connection lost-------------> seat freed; auto-reconnect attempts a fresh join

Ranked-queue --match found----------> Game (seated; matchmaker room, auto-starts, no host)
Ranked-queue --cancel---------------> Menu
Ranked-queue --aborted (too few)----> Menu (with a re-queue handle)

Game --ends-------------------------> Game/Result
Game --resign-----------------------> Game (as eliminated-watcher; still on this surface)
Game --leave (as watcher)-----------> Menu
Game --connection lost--------------> Game/Disconnected -> auto-reconnect -> Game/Reorienting -> live
Game/Result --play again (plain)----> Game (same room restarts for connected seats)
Game/Result --back to queue (ranked)-> Ranked-queue
Game/Result --leave-----------------> Menu
```

---

# Impossible transitions

- Reaching Menu, Lobby, Ranked-queue, or Game without first establishing a
  connection and identity at Connect.
- Menu -> Game or Menu -> Lobby without creating, joining, or matching into a
  room (there is no direct "enter a game" from the hub).
- Menu -> Ranked-queue as a guest, or when the server does not offer ranked
  (ranked is keyed to a token identity and to a server that enables it).
- Lobby -> Game without the host starting, with at least two seats, and valid
  rules (a start that fails validation stays in the Lobby).
- Spectator (in Game) -> Lobby, or spectator -> acting: a watcher holds no seat
  and can never become a seated player in that game.
- Rules -> Game or Rules -> Lobby: Rules is a reference leaf and only returns to
  Menu.
- Game/Result -> Lobby: play-again re-enters Game directly; the lobby no longer
  exists.
- Game -> a second Game: a player holds at most one seat in one room at a time;
  entering another game requires leaving the first.
- Game/Reorienting -> acting before reorientation completes.
- Any surface -> skipping Connect on a cold start.

---

# Back navigation

The application is a shallow spine with one branch, not a deep stack; "back"
means "return to the surface that owns the exit", not "pop an arbitrary history".

- **Connect** is the root; it has no back (its only backward move is closing the
  application).
- **Menu** returns to **Connect** by disconnecting.
- **Rules**, **Server-manager**, **LAN-discovery** return to **Menu**.
- **Lobby** returns to **Menu** by leaving (the connection stays usable to create
  or join again).
- **Ranked-queue** returns to **Menu** by cancelling.
- **Game** does not return mid-play by a back gesture: leaving a game is an
  explicit act (resign, then leave as a watcher; or leave from `Result`), never
  an accidental back. A within-game "back" gesture is reserved for dismissing a
  contextual decision, and it never leaves the surface (see `GAME_SCREEN.md`).
- Every backward move lands on a surface that offers a next action; no backward
  move produces a dead end (an aborted queue hands back a re-queue; a dissolved
  room hands back the Menu; a lost connection hands back reconnection or the
  Menu).

---

# Deep links

- **The room code is the peer deep-link.** A room's code is a shareable token;
  another player reaches the same Lobby by entering that code at Menu -> join.
  This is the only sharing-based navigation into a specific room, and it is
  by-code, not by-URL.
- **The server origin is an implicit deep-link on the web.** On the web the
  connection target is derived from the address the application was opened at, so
  the address itself selects the server; a liveness probe resolves whether that
  server is reachable and whether it admits guests.
- **The sign-in return is a deep-link-like handoff**, not app navigation: the
  external authority returns to the Sign-in sub-flow carrying a token, which
  returns to Connect.
- **Not currently a mechanic:** a URL that auto-navigates into join/lobby with a
  code pre-filled, or that resumes a specific prior game, does not exist; a code
  is entered, not followed from a link. If such a URL deep-link is ever wanted it
  is a new capability, and only then part of this specification.

---

# Interruptions

Events that reroute the player regardless of intent:

- **Connection lost** (any connected surface): Game -> Disconnected then
  auto-reconnect; Lobby -> the seat is freed and auto-reconnect attempts a fresh
  join; Menu/Connect -> return toward Connect. Never a silent dead end.
- **Server unreachable** (at Connect): stays at Connect with a liveness signal;
  it never navigates the player away.
- **Match found** (Ranked-queue): auto-navigates into Game; the wait ends without
  a player action.
- **Room dissolved** (Lobby): returns to Menu.
- **Game ended / time expired** (Game): enters `Result`.
- **Eliminated** (Game): becomes an eliminated-watcher on the SAME surface; no
  navigation away until the player leaves.
- **Queue aborted** (Ranked-queue): returns to Menu with a re-queue handle.
- **Turn auto-played** (Game, AFK/disconnected): no navigation; the player stays
  on the surface, and the auto-play is made known.

---

# Reconnection flows

- **Socket drop during a live game** (within a session): the surface enters
  Disconnected; the client auto-reconnects using the per-seat credential it was
  issued when it joined; the last connection for an identity wins (a second
  connection takes the seat over); on success the surface enters Reorienting -
  snap to the true present, no replay of missed history - then the live game
  state. The game continued in the player's absence, and their turns were
  auto-played after a grace period.
- **Socket drop while assembling** (Lobby): the seat was freed on disconnect, so
  reconnection is a FRESH join, not a seat restore - it succeeds if the room
  still exists with room, and otherwise returns the player to Menu with a
  message.
- **Socket drop at Menu/Connect**: return toward Connect and re-establish; no
  game state is at risk.
- **Cold application restart**: begins at Connect; there is no cross-restart
  auto-rejoin of a prior game - a game is re-entered, if at all, by navigating
  Connect -> Menu and joining again while the seat is still held server-side.

---

# Game-creation flows

- **Create a private room:** Menu -> create (server-default content) -> Lobby as
  host. A modded variant chooses the content set at creation; the content set is
  fixed for the room's life.
- **Join a private room:** Menu -> join by code -> Lobby as member (or refused if
  the code is invalid, or the room is full of humans).
- **Ranked:** Menu -> Ranked-queue (only with a token, only where the server
  offers ranked) -> match found -> Game, in a matchmaker-created room that
  auto-starts and has no host.
- In every creation path, the game itself begins only when the room launches (a
  host start in a private room, an automatic start in a matched room).

---

# Result flows

- A game reaches `Result` on a terminal event; the table stops and only
  next-action choices remain.
- **Play again** (plain room): the same room restarts for the still-connected
  seats and re-enters Game; seats that left are dropped; an eliminated but
  connected player may take part in the restart.
- **Back to queue** (ranked): a ranked result offers no in-place replay; it
  returns the player to Ranked-queue (a ranked outcome also carries the rating
  change before this choice).
- **Leave:** returns to Menu.
- A player who was eliminated earlier reaches `Result` on the same surface and
  chooses among the same next actions their standing allows.

---

# Functional Guarantees

- **No dead ends.** Every surface, and every interruption, leaves the player on a
  surface that offers a next action; a refused move (bad code, full room, invalid
  launch, unreachable server) is a no-op that keeps the player where they are
  with a reason, never a stranding.
- **One live game at a time.** A player occupies at most one game; navigation
  never places them in two.
- **Connection precedes everything.** No surface past Connect is reachable
  without a connection and identity, and losing them routes back toward Connect
  rather than into an unusable state.
- **Explicit exit from play.** Leaving a live game is always an explicit act,
  never an accidental back gesture.
- **Deterministic hand-off.** A room launch, a match, and a play-again each hand
  every affected participant into the game together; a dissolution or a leave
  returns each to the Menu; there is no partial or split navigation.

---

# Out of Scope

This document never describes, and a reader must never infer from it:

- any control, gesture, or navigation affordance's appearance or position;
- any widget, component, layout, or design system;
- any colour, typography, iconography, animation, timing, or sound;
- any framework, routing implementation, or transport detail;
- the internal behaviour of a surface (owned by that surface's SPECS document);
- any feeling or emotional claim (owned by `DESIGN/PLAYER_EXPERIENCE`);
- any rule definition (owned by the rules and engine documents).

---

# Phase 4 - Self-critique

- **This specification merits existing, and it was the missing owner.** The
  inter-surface transition graph - entry/exit conditions, impossible moves,
  reconnection and result routing - had no owner: `DESIGN/SCREEN_ARCHITECTURE`
  holds the one-line spine and per-screen navigation AFFORDANCES (placement),
  `DESIGN/product/` is cross-surface behaviour with no navigation, and the
  per-surface SPECS own only their internal state machines. This document owns
  the graph between them. Created as `SPECS/NAVIGATION.md`.
- **Boundary honesty (vs SCREEN_ARCHITECTURE).** SCREEN_ARCHITECTURE mentions
  navigation as affordance ("leave is explicit", "a dead end returns a handle",
  "Escape does not leave mid-game"); this document states the same facts as
  TRANSITIONS and IMPOSSIBLE transitions, without any affordance. The overlap is
  a shared derivation from the mechanics, not a duplication of ownership - as
  long as SCREEN_ARCHITECTURE never enumerates the transition graph and this
  document never places a control. The one phrase that recurs ("a dead end
  returns a handle") is a guarantee here and an affordance there; acceptable,
  flagged.
- **Truthful about what does not exist.** URL-level deep links (auto-navigate to
  a room from a link, resume a prior game across restarts) are NOT current
  mechanics; the room code and the web origin are the only real deep-links.
  Specified as absent rather than invented.
- **Result is a state, not a surface.** Kept consistent with `GAME_SCREEN.md`
  (Result is Game's terminal state); SCREEN_ARCHITECTURE names a "Finished"
  screen for placement purposes - the same surface, its terminal state - and
  this document does not create a competing surface.
- **Asymmetry carried through.** The lobby-frees-seat vs game-holds-seat rule
  (from `LOBBY_SCREEN.md`/`GAME_SCREEN.md`) drives the two different reconnection
  flows; stated explicitly so it cannot be assumed uniform.
- **Impossible transitions checked, not assumed.** Guest-into-ranked,
  spectator-into-seat, rules-into-game, result-into-lobby, and two-games-at-once
  are each derived from a real constraint (ranked keyed to a token; a watcher
  holds no seat; rules is a leaf; the lobby is gone at result; one seat per
  player), not asserted for neatness.
- **Residual risk.** Like the other SPECS documents, this one is coupled to the
  server's room and session behaviour (auto-reconnect with a token, matchmaker
  auto-start, dissolution, last-connection-wins). If any of those change, the
  affected transitions change with them - which makes the graph checkable against
  the server rather than trusted as prose.
- **Convertibility test applied.** Every statement is a transition, a condition,
  or a guarantee; none names a control, a position, a colour, or a motion. A
  paper wireframe of the flow and a live application would satisfy it identically.
