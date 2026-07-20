# Ranked & spectate summary

Sources of truth: ADR-0034, ADR-0035, crates/server/src/{ranked/,
showcase.rs, room.rs, ws.rs}, crates/engine/src/view.rs.

## Ranked (ADR-0034) - a PER-SERVER ladder

Why per-server: community hosts are untrusted; a global ladder needs
signed results (explicitly deferred). A cheating host only corrupts
their own ladder.

- **Identity**: ratings key on the token `sub`-derived `player_id`;
  NEVER the mutable handle; guests (spoofable) are refused at
  `queue_ranked` and `get_rating`.
- **Rating math** (`ranked/ladder.rs`, pure): Weng-Lin via the
  `skillratings` crate (chosen over patented TrueSkill and 2-player
  Glicko-2). mu=25, sigma=25/3 start; display =
  `max(0, 1000 + 40*(mu - 3*sigma))`.
- **Queue** (`ranked/queue.rs`): widening window (2.0 mu + wait/10s,
  min of both sides), target 4 seats, any 2 after a 60s fallback,
  2s matchmaker tick; entries are connection-scoped (re-queue replaces;
  removal checks `same_channel` so an old connection can't evict a new
  entry).
- **Rooms**: matchmaker-created, server-default settings, only matched
  identities may join, NO host powers (Configure/bots/Start/PlayAgain
  rejected), auto-start when all arrive or after a 15s grace with >=2
  (fewer aborts + dissolves).
- **Result**: at Finished, placements = winner first, survivors by VP
  then net worth then lowest seat, eliminated in reverse fall order;
  one `RatingStore.record_match` (audit rows in `rated_game`);
  `ratings_updated` broadcast.
- **Persistence**: `--history` DB file (own connection) or in-memory
  with a boot warning.
- **Known-accepted rough edges**: anchor head-of-line blocking; queued
  {size} doubles as cancel ack; re-queue resets waiting credit
  (technical-debt.md D5).

## Spectate (ADR-0035)

- **View**: `ClientView::for_spectator` - NO trade offers, ALL pending
  bids/votes masked; resolution events public as for everyone; trade
  lifecycle events filtered from the spectator feed (`event_public`).
- **Wire**: `spectate {code?, auth}` (same auth as join; no code =
  server probes rooms and picks the Active game with most connected
  humans - which degrades to the showcase) -> `spectating {...}` (a
  seatless `joined`).
- **Sessions**: spectators may only watch and leave; game commands
  refused at transport; `animation_done` dropped silently (they never
  gate timers); addressed by unique `watch:<n>:<id>` keys so their
  disconnects can't shadow seats; cap 32/room; dead senders pruned.
- **Liveness**: spectators DO keep a room from idle-dissolving;
  `RoomCmd::Probe` does NOT count as activity (the rule that keeps the
  15s supervisor from immortalizing every room - do not "simplify" it).

## Bots showcase (`--showcase`, off by default)

Supervisor task, 15s tick: if no room has an Active game with a
connected human AND no showcase exists, create one - four bot seats,
born Active, replays itself 10s after each finish, never joinable.
Winds down via normal idle timeout once unwatched. Purpose: `spectate`
always finds something; watching bots is half the onboarding story.
