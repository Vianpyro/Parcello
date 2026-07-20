# Performance

Where the cycles and bytes actually go, what scales and what doesn't,
and - most importantly - what must NOT be optimized. This project's
performance posture is deliberate modesty: a board game with <=6 seats
per room and human-paced turns has no hot loop, and premature
optimization here would trade away the purity and clone-based
correctness model that everything else stands on.

## Current profile (measured by reasoning, not by profiler - see end)

- **Engine**: `apply` clones `GameState` (a few KB: vectors of players,
  tiles, decks) once per accepted command. Commands arrive at human
  cadence (or 1/800ms per bot). This is nothing.
- **Fan-out**: the real per-command cost is building N per-seat
  `ClientView`s + 1 spectator view and serializing each to JSON
  (`send_per_seat` / `send_spectators` in room.rs). O(seats +
  spectators) serializations of a view that is itself O(tiles+players).
  At 6 seats + 32 spectators on a 32-tile board this is tens of
  kilobytes per command - still trivial at turn cadence.
- **Persistence**: history writes go through a dedicated writer thread
  (never blocks a room, ADR-0005); rating updates are one small SQLite
  transaction per FINISHED game (documented exception, ADR-0034).
- **Timers**: `Room::run` recomputes ~8 `sleep_until` targets per loop
  iteration; iterations happen per-message or per-timer-fire. Cheap,
  and the recompute-each-loop shape is what makes mid-turn disconnects
  shorten deadlines correctly - do not cache it away.
- **Background tasks**: ranked matchmaker (2s tick over a usually-empty
  queue), showcase supervisor (15s tick probing every room: O(rooms)
  message round-trips), LAN announcer (opt-in UDP). All negligible.
- **Startup**: mod resolution + engine validation once at boot and once
  per custom-mod room creation (spawn_blocking'd because it is real
  filesystem I/O).

## The scaling model (important)

This server scales like Minecraft, not like an MMO: **one modest process
per community, many communities**. Horizontal scaling of a single
server, shared-state clustering, and Postgres are all non-goals
(architecture.typ). Before "scaling" anything, re-read that trade-off:
the answer to a big community is a second server, not a bigger one.

Rough single-process envelope (unmeasured, but bounded by design):
`MAX_CONNECTIONS` = 1024 sockets caps everything above it. A hundred
concurrent rooms of bots-speed traffic is well inside what one core
handles; memory is dominated by per-room `GameState` + content Arcs
(shared) - megabytes, not gigabytes.

## Latency: the truth about "feel"

Perceived latency is dominated by the ANIMATION CONTRACT, not the
network. An Update is rendered as paced beats (up to 8s budget) and the
server's decision windows only start once the table has visually
arrived (ADR-0028/0030). Optimizing network round-trips below ~100ms
buys nothing a player can feel; breaking the watermark to "make it
snappier" desynchronizes the table. Tune game feel in
`docs/motion-language.md` terms, not in milliseconds of transport.

## What must never be "optimized" (anti-optimization list)

1. **Engine clone-per-apply** -> in-place mutation with rollback. The
   clone IS the rejection-atomicity guarantee (E2). Any rollback scheme
   trades a correctness invariant for microseconds nobody can perceive.
2. **Per-seat view rebuilds** -> cached/diffed views. Masking (E5) is
   per-seat; a shared cached view is how hidden bids leak. Diffing adds
   a protocol format (P1 break) for bandwidth nobody is short of.
3. **`group_tiles` lazy iterator** -> precomputed group tables. It is
   already O(board) per landing on a 32-tile board; a cache adds
   invalidation logic to the engine for nothing.
4. **JSON -> binary protocol.** JSON is the replay format, is
   debuggable with a text editor, and is generated/parsed by three
   codebases. Bandwidth per game is kilobytes. (Compression at the
   proxy is free if ever wanted.)
5. **The timer recompute loop** -> event-driven deadline registry. The
   loop shape is load-bearing for correctness (see above) and its cost
   is unmeasurable.
6. **SQLite -> anything** before a measured need. ADR-0005 already
   maps the exit (SQLx adapter behind the same trait) if dashboards
   ever need rich queries.

## Plausible future bottlenecks (in order of likelihood, with triggers)

1. **Spectator fan-out on a popular room**: 32 spectators x per-command
   serialization. Trigger: profiling shows serialization hot AND
   popular rooms exist. Fix shape: serialize the spectator view ONCE
   per update (it is identical for all watchers) - a contained change
   inside `send_spectators`; do NOT share seat views.
2. **Showcase probing at high room counts**: O(rooms) round-trips per
   15s tick. Trigger: hundreds of rooms. Fix shape: a shared atomic
   "active humans" counter maintained by rooms, replacing probes -
   needs care with the S6 activity rule.
3. **`Rooms` RwLock churn**: every create/join/probe takes it. Trigger:
   thousands of connection events/sec (i.e., not this game). DashMap is
   the boring fix; don't take it early.
4. **Flutter web canvas on low-end hardware**: already mitigated by the
   animation budget and reduced-motion profile (ADR-0030); measure on a
   Steam Deck before touching code.

## When someone finally profiles

There are no benches in the workspace ON PURPOSE (CLAUDE.md notes CI
has no bench story). The first legitimate one arrives with the first
measured complaint, and it should be criterion + a fixed seed replaying
a recorded command log through `Engine::apply` - the only code path
where "fast" could ever matter (bulk replay/verification tooling).
Anything else, measure in situ first (tokio-console or `tracing` spans
- the server already uses `tracing` everywhere).
