# ADR-0034: ranked matchmaking with a per-server ladder

Status: accepted

## Context

The client's main menu reserves a "Play (ranked)" tile, greyed until a
matchmaking service exists (`docs/visual-identity.md`), and the player card
is waiting for an MMR number. `docs/architecture.typ` section 8 lists "no
central matchmaking" as an accepted trade-off of the Minecraft hosting
model, and ADR-0009 explicitly defers stats submission because an untrusted
community server could forge results. Any *global* ladder therefore needs a
developer-hosted rating authority and signed match results - a separate
project. Deviating from "no central matchmaking" at all requires this ADR.

## Decision

**The ladder is per server.** Each game server owns its own ranked queue and
rating table, stored locally. A community server can only corrupt its own
ladder, which is its own reputation - consistent with the hosting model.
The operator's public server is the reference ladder; a future global
aggregation layer stays deferred with ADR-0009's stats note.

**Identity.** Ratings key on the stable `player_id` (the token `sub`,
ADR-0009), never the mutable display name (ADR-0033). Spoofable identities
(guests, ADR-0003/0008) cannot enter the queue: an unforgeable identity is
the price of a persistent rating.

**Rating system: Weng-Lin (JMLR 2011, the OpenSkill model family),** via the
`skillratings` crate (MIT OR Apache-2.0, no mandatory runtime deps).
Reasoning: Elo and Glicko-2 are two-player systems that need pairwise
decomposition for a 2..=6 free-for-all and (Elo) track no uncertainty;
TrueSkill is multiplayer-native but patented, and commercial distribution
is planned. Weng-Lin is multiplayer-native, closed-form, patent-free, and
tracks `mu`/`sigma` per player (new players start at mu=25, sigma=25/3).
The displayed number is `max(0, 1000 + 40 * (mu - 3*sigma))` rounded - a
conservative ordinal, so a new player shows 1000 and climbs as sigma
shrinks. Tiers/leagues stay deferred (`docs/visual-identity.md`).

**Placements.** A rated game yields a strict best-to-worst ordering of the
seats, derived in the session layer (the engine is untouched):

1. the engine's declared winner is always placement 1;
2. remaining survivors ordered by victory points, then net worth, then
   lowest seat (the house tie-break) - the metrics of ADR-0010/0020;
3. eliminated seats below all survivors, in reverse elimination order
   (later bankruptcy/resignation places higher). The room records the
   elimination order from `PlayerBankrupt`/`PlayerResigned` events as they
   are emitted.

**Storage: a new `RatingStore` port**, not `GameHistory`. `GameHistory` is
append-only and fire-and-forget by contract (ADR-0005); ratings are
read-modify-write (read at queue entry, update at game end). A second
Repository trait is the architecture's own sanctioned pattern, not a new
seam. Adapters: in-memory (tests, and ranked without persistence - the
server warns that the ladder dies with the process) and rusqlite in the
same database file as `--history` (its own connection; WAL makes the two
connections safe). The end-of-game update is one small synchronous
transaction per finished game - deliberately NOT the enqueue pattern of
ADR-0005, which exists for the per-command hot path; a per-game write that
must return the new ratings for broadcast does not justify a writer thread.

**Queue protocol** rides the existing WebSocket, additive messages only:

- `queue_ranked {auth}` / `cancel_queue` - connection-scoped like
  `list_mods` (a queued connection is in no room); creating or joining a
  room, cancelling, or disconnecting removes the entry. One entry per
  identity.
- `get_rating {auth}` -> `rating {...}` - feeds the menu player card.
- `queued {size}` - queue-size updates to waiting members.
- `match_found {code}` - the client answers with a normal `join`. The
  transport stays dumb (architecture section 5.1): the server never
  teleports a connection into a room.
- `ratings_updated {changes}` - broadcast to the room when a rated game
  ends; each change carries `player_id`, new `mu`/`sigma`, the display
  value and its delta.

**Matchmaker.** A single server task ticks every 2s over the queue. Each
entry's tolerance window is `2.0 + waited_secs / 10` in mu units; two
entries are compatible when `|mu_a - mu_b| <= min(w_a, w_b)`. The oldest
entry anchors: if `RANKED_TARGET_SEATS` (4) compatible entries exist, they
match (closest mu first); after the anchor has waited
`QUEUE_FALLBACK_SECS` (60s), any `MIN_PLAYERS` (2) compatible entries
match. Constants, not flags, until playtests say otherwise.

**Ranked rooms** are ordinary rooms with a `ranked` marker, created by the
matchmaker with the server's default settings (time-boxed by default, so a
rated game always ends):

- only the matched identities may join (rejoin by identity keeps working);
- no host powers: `Configure`, `AddBot`, `RemoveBot`, `Start`, and
  `PlayAgain` are rejected (replay goes back through the queue);
- the room auto-starts when every matched player has joined, or after
  `RANKED_START_GRACE` (15s) with at least `MIN_PLAYERS` present (absent
  seats are dropped unrated); with fewer, it aborts and dissolves;
- at `Finished`, the room computes placements, applies the Weng-Lin update
  through `RatingStore`, and broadcasts `ratings_updated`.

Enabled by the `--ranked` flag, off by default.

## Consequences

- The engine, the replay format, and the game rules are untouched; all of
  this lives in the session layer plus one new port.
- The wire grows only additive, omit-when-unset shapes; old clients ignore
  them.
- A player who never joins after `match_found` wastes at most 15 seconds
  of the others' time; leaver penalties, seasons/decay, party queueing and
  the global ladder are explicitly deferred.
- The Flutter ranked UI (un-greying the tile, queue screen, MMR on the
  player card) is a follow-up task; the CLI gets `queue`/`rating` commands
  now as the cheapest end-to-end check.
