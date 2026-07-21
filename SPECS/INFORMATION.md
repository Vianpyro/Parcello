# Information Cartography - specification

A SPECS document (see `SPECS/README.md`): observable functional behaviour only.
It maps every piece of information Parcello holds - for each datum, who PRODUCES
it, who may CONSUME it, how often it updates, how long it lives, whether it
persists, its confidentiality, when it first appears, and what it depends on. It
describes no interface.

This is the STATE map (the nouns). Its two siblings describe other axes and are
referenced, not restated:

- `SPECS/EVENTS.md` - the TRANSITIONS (the verbs): the facts that CHANGE this
  information.
- `DESIGN/product/INFORMATION_ARCHITECTURE.md` - the player's COGNITIVE model
  (what an interface must present, permanent vs contextual, never hidden). That
  is a requirement on presentation; THIS document is the datum's provenance and
  confidentiality, regardless of any interface.

Each datum is specified by the eight fields requested. To avoid repeating the
constant fields on every row, each FAMILY states its defaults; a datum's row
lists the discriminating fields (update frequency, lifetime, confidentiality,
dependencies) and any override.

---

# Confidentiality tiers (used throughout)

A single taxonomy of who may see a datum's value:

- **Public** - visible to every participant and every spectator.
- **Masked-until-reveal** - hidden while in flight, disclosed to all at a single
  resolution moment (a sealed bid, a pending vote).
- **Private-to-parties** - visible only to the specific participants it concerns
  (a trade offer's two sides).
- **Never-exposed** - held by the producer and placed in NO consumer's view
  (the randomness source, undrawn order, the market generator).
- **Shareable-secret** - not published, but whoever holds it gains access (a room
  code: possessing it lets one join).
- **Per-holder-secret** - issued to exactly one holder and must not leak (a
  seat's reconnect credential; an identity token).
- **Device-local** - never leaves the participant's own device.

Confidentiality is a functional property of the datum (who may KNOW its value),
never a presentational one.

---

# Family A - Game state

**Defaults.** Producer: the game engine. Consumers: every seated participant's
per-seat view and (public parts only) the spectator view. Persistence:
reproducible from the persisted, ordered log of accepted commands plus the seed;
not stored per-value. Order of appearance: present from the game's first moment
(initial state) unless noted. Dependencies: the accepted-command stream via the
events that change it (`SPECS/EVENTS.md`).

| Datum | Update frequency | Lifetime | Confidentiality | Depends on |
|---|---|---|---|---|
| Pawn positions | per move | the game | Public | movement events |
| Property ownership / mortgage / development | per transfer, build, mortgage | the game | Public | auction, trade, expropriation, bankruptcy, build |
| Each player's cash | per cash event | the game | **Public** (by design) | rent, salary, tax, build, auction, trade |
| Each player's victory points | per holdings change and per round bonus | the game | Public | ownership + the round metronome |
| Each player's net worth (derived) | continuously (cash + holdings) | the game | Public (decides a timed win) | cash + ownership |
| Each player's hand of movement values | per play and per refill | the game | **Public** (open hands) | plays + refills |
| Player status (alive / jailed / route / jail-cards / rounds cycled) | per relevant event | the game | Public | jail, movement, refill events |
| Current turn / phase / acting seat | per turn or phase change | the game | Public | turn flow |
| Market forecast (scheduled + active events) | per schedule advance | the game | **Public** (the drawn schedule, never the generator) | the seeded schedule |
| Spotlight (spotlit tile + terms) | per exposition landing / expiry | the game | Public | exposition landings |
| Shared building pools (available levels) | per build / liquidation | the game | Public | builds + seat count |
| Sealed bids in flight | per submission | the auction window | **Masked-until-reveal** (own bid to self; others see only committed/not) | an open auction |
| Pending bribe votes in flight | per cast | the vote window | **Masked-until-reveal** | an open vote |
| Trade offers pending | per trade lifecycle event | until resolved / withdrawn / stale | **Private-to-parties** | trade commands |
| The randomness source / undrawn order / market generator | continuous internally | the game | **Never-exposed** | the seed alone |
| The consequence (event) stream | per event | the game | Public, with masked events staying masked | all events |

**The one non-deterministic datum - the clocks.** The game clock, the per-turn
clock, the time bank, and the auction/vote window clocks are produced by
real-time timers, consumed by all relevant views, updated continuously, live for
the game or the window, are **Public**, and depend on the frozen settings plus
wall-clock time. Unlike everything else in this family, their exact readings are
NOT reproducible from the command log - they are real-time, not deterministic
state. The DURATIONS are frozen settings (deterministic); the elapsed VALUE is
not. This is the single datum a replay cannot reproduce bit-identically.

---

# Family B - Room / lobby state

**Defaults.** Producer: the room/session layer (server). Consumers: the room's
participants (and, for public parts, its spectators). Persistence: NOT part of
the game replay; server-held for the room's life. Dependencies: the room events
(`SPECS/EVENTS.md`, session family). Order: from room creation unless noted.

| Datum | Update frequency | Lifetime | Confidentiality | Depends on |
|---|---|---|---|---|
| Room code | once, at creation | the room | **Shareable-secret** (holding it grants join) | room creation |
| Roster (seats: identity, is-bot, connected) | per join / leave / bot | the room | Public within the room | join/leave/bot events |
| Host identity (= first-seat occupant) | per compaction | the room | Public | roster |
| Room settings (clocks + rules) | per host change; frozen at launch | the room, then frozen into the game | Public (house rules on the table) | host changes + server defaults |
| Display handle per seat | at join | the room / game | Public (the handle; the underlying identity is not) | the player's chosen handle |
| Per-seat reconnect credential | once, at join | the seat's life | **Per-holder-secret** (spoofable seats depend on it) | the seat |

---

# Family C - Session / client-local

**Defaults.** Producer: the connecting client and its chosen server/identity
provider. Consumers: the client itself, plus what it sends to the server to
authenticate/connect. Persistence: held on the participant's own device across
sessions where noted. Order: at connect / sign-in.

| Datum | Update frequency | Lifetime | Confidentiality | Depends on |
|---|---|---|---|---|
| Server address + runtime config (guest-allowed, issuer) | on connect / probe | the session | Public (server config) | the chosen server |
| Identity token (guest or signed-in) | on sign-in | the session | **Per-holder-secret** (it authenticates) | the auth flow |
| Saved issuer / onboarding-hints-seen / locale | on change | across sessions (local) | **Device-local** | the participant's own actions |

---

# Family D - Persistent, cross-game (server)

**Defaults.** Producer: the server's stores. Consumers: server pipelines and the
participant at the relevant moment. Persistence: STORED and outliving any single
game. Order: after the game or action that yields it.

| Datum | Update frequency | Lifetime | Confidentiality | Depends on |
|---|---|---|---|---|
| Ratings (per-server ladder) | per ranked game end | persistent across games | a local reputation, keyed to the identity (never the handle, never guests) | ranked game outcomes |
| Game history records | per completed game / per feedback | persistent | server-held | completed games + feedback |
| Post-game feedback (rating + comment) | once per seat, at game end | persistent (sanitized) | server-held | the seated participant's submission |

---

# Cross-cutting properties

- **Determinism boundary.** Everything in Family A except the clocks is a pure
  function of the initial participants, the seed, and the ordered accepted
  commands, so it is reproducible without being stored. The clocks are the
  exception (real-time). Families B, C, and D are NOT part of that replay.
- **Confidentiality is enforced at production, not presentation.** A masked or
  private datum is withheld from a consumer's view by the producer; no consumer
  is trusted to hide what it was given. This is why sealed bids and private
  trades are safe regardless of any interface.
- **Order of appearance follows the events.** A datum first appears when the
  event that creates it first fires (`SPECS/EVENTS.md`): game state at the
  initial state; room state at creation; ranked/history/feedback after their
  producing game.
- **Dependencies form the same acyclic flow as the events.** Derived data (net
  worth, victory points, the round leader) depend on base data (cash, ownership,
  rounds cycled); no datum depends on a datum that depends on it.

---

# Out of Scope

This document never describes, and a reader must never infer from it:

- any interface, control, layout, colour, typography, animation, or sound;
- any wire format, storage schema, type, or field name (owned by the protocol,
  engine, and history documents);
- any framework or implementation detail;
- the player's cognitive requirements on this information (owned by
  `DESIGN/product/INFORMATION_ARCHITECTURE.md`);
- the events that change this information beyond naming them as dependencies
  (owned by `SPECS/EVENTS.md`);
- rule definitions or numeric values (owned by the rules and engine documents).

---

# Phase 4 - Self-critique

- **This layer merits existing, and it is a third, distinct axis.** The provenance
  map - producer, consumers, lifetime, persistence, confidentiality, dependencies
  per datum - was un-owned: `product/INFORMATION_ARCHITECTURE` owns the player's
  cognitive requirement (present / permanent / never-hidden), `SPECS/EVENTS` owns
  the transitions, and the engine/view/history own the mechanism. None maps the
  static information's flow and secrecy as a whole. Created as
  `SPECS/INFORMATION.md`.
- **Boundary honesty (vs product/IA and vs EVENTS).** The overlap is
  "confidentiality" vs "never hidden" vs "event visibility". They are three
  angles on secrecy: product/IA states a REQUIREMENT (the player must always have
  X); EVENTS states an event's VISIBILITY (who may know a fact happened); this
  document states a datum's CONFIDENTIALITY (who may see its current value). I
  gave confidentiality a single tier taxonomy here and referenced the others
  rather than restating them; the residual risk is that the three drift, so each
  points at the others.
- **The one genuinely new finding: the clocks break determinism.** Every other
  piece of game state is reproducible from the log+seed; the clock READINGS are
  real-time and are not. Stated as the single non-deterministic datum, because a
  reimplementation that assumes "all game state replays" would be wrong about
  exactly this one, and it matters for replays and spectators.
- **Four confidentiality tiers that are easy to conflate are separated.**
  Shareable-secret (the room code - possessing it grants access), per-holder-
  secret (a reconnect credential / token - must not leak), never-exposed (the
  randomness source - in no view at all), and private-to-parties (a trade). A
  reimplementation that treated the room code like a token, or a bid like a
  trade, would leak; the tiers make the distinction explicit.
- **Completeness check.** Game state was derived from the engine's per-seat view
  and its masked members (randomness, sealed bids, pending votes, private
  trades); room, session, and persistent families cover the non-replay data
  (code, roster, host, settings, handles, reconnect credentials, tokens, local
  preferences, ratings, history, feedback). The catalogue is complete to the
  functional data Parcello holds; anything not listed is derivable from a listed
  datum.
- **Residual risk.** Like its siblings, this is coupled to the engine's view
  masking and the server's stores; if masking or persistence changes, the
  affected rows change with it - which makes the confidentiality map directly
  checkable against what each view actually exposes rather than trusted as prose.
- **Convertibility test.** Every field is a fact about a datum (who makes it, who
  may see it, how long it lives), never a control or a position; a paper data-map
  and the running system would agree on all of it.
