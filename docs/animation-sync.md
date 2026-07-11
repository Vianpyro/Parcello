# Design direction: animation-aware timing (backend/frontend sync)

Status: DECIDED and implemented - see ADR-0028 (animation-ack watermark).
Option D below was adopted; the open questions were settled with the
owner during the first playtests (2026-07): table-wide pause for the
collection windows, the turn clock also gated (on the acting seat's own
ack), hard cap 6s, `WentToJail` enriched with `from`, the absolute game
clock untouched. This document stays as the design record.

## Problem

Today the server is animation-blind by design. `Engine::apply` is pure and
clockless (hard constraint, `docs/architecture.typ` section 4); the Session
Layer's timers (`bid_deadline`, `afk_deadline`/`turn_seconds`, `BOT_THINK`,
`game_deadline`) are all anchored to wall-clock instants
(`crates/server/src/room.rs`) with zero awareness of what a client is
currently rendering. Clients receive a full burst of `Event`s in one
`ServerMessage::Update` per accepted command and are free to animate them
however and however fast they like - but nothing stops the _next_
server-driven thing (a bot's move, an auction window opening, an AFK
auto-play) from firing before a human has visually finished watching the
current one.

Three concrete symptoms motivate this (as raised in conversation,
2026-07):

1. **Timers should pause during animations.** A per-turn/AFK countdown or
   the sealed-bid `BID_WINDOW` (`crates/server/src/room.rs:46`) ticking
   down in real wall-clock time while a client is mid-animation eats into
   the player's real thinking time for no reason.
2. **Time-sensitive phase transitions should wait for visual arrival.**
   `BlindAuctionOpened` (`crates/engine/src/event.rs:39`) is emitted in the
   _same_ `Update` batch as the `Moved` event that puts the discoverer on
   the tile (`crates/engine/src/apply.rs:927` `resolve_landing`,
   `crates/engine/src/apply.rs:953` opens the auction). Today a client
   could show the bid overlay before the token has visually finished
   sliding onto the tile. Symmetrically, nothing currently blocks
   `EndTurn` from being sent (or auto-played) before a player's own
   movement animation has settled.
3. **Multi-hop chains need per-hop pacing.** Landing on Chance, drawing a
   "go to jail" card, and being teleported to Jail is _already_ multiple
   discrete engine events - `Moved` (old position -> Chance tile),
   `CardDrawn` (`crates/engine/src/apply.rs:1011`), then `go_to_jail`
   (`crates/engine/src/apply.rs:899`) - but they all land in one `Update`
   burst with no pacing hint, so a naive client either snaps instantly
   through all three or has to hardcode per-card-type animation
   heuristics.

## What already exists (audit, so a future implementer starts from facts)

- **Event granularity is mostly already there.** `Moved { player, from,
to, passed_go }` (`crates/engine/src/event.rs:26`) carries both
  endpoints, which is what per-hop animation needs. Card chains
  (`CardEffect::MoveTo`/`MoveBy`) recurse through `resolve_landing` up to
  `MAX_CARD_CHAIN_DEPTH = 4` (`crates/engine/src/apply.rs:17`), so a
  "card sends you to another card tile" chain already produces an ordered
  sequence of `Moved`/`CardDrawn` events in one `Vec<Event>` - this is the
  existing precedent to generalize, not a new mechanism to invent.
- **One gap found while auditing:** `WentToJail { player }`
  (`crates/engine/src/event.rs:148`) does _not_ carry `from`, unlike
  `Moved`. `go_to_jail` (`crates/engine/src/apply.rs:899`) sets the
  position directly and never pushes a `Moved` event for the jail hop. A
  client can still animate this hop today only by remembering the
  player's last known position itself. Whether to enrich the event
  (`WentToJail { player, from }`) or keep relying on client-side
  last-known-position is an open question below - flagging it now so it
  is not rediscovered mid-implementation.
- **Delivery is batch-atomic, not incremental.** `ServerMessage::Update`
  (`crates/protocol/src/lib.rs:176`) carries the _entire_ `Vec<Event>`
  produced by one `apply()` call. There is no per-event or per-hop
  delivery today, and no sequence/ack concept on the wire at all.
- **Server timers are wall-clock only.** `last_progress`, `bid_deadline`,
  `BOT_THINK`/`bot_think_delay()`, `afk_deadline`, `game_deadline` all live
  in `crates/server/src/room.rs` and are computed from
  `tokio::time::Instant`, with no hook for "client X is still animating".
  This is the piece that needs a new primitive; see options below.
- **The "timed collection window" primitive already exists once.**
  ADR-0018's sealed-bid window (arm a deadline, collect one submission per
  seat, inject canonicals at expiry, resolve on the last one) is
  explicitly noted there as reusable ("written once and reused by the
  corruption vote", ADR-0024). An animation-ack window is the same shape
  (arm a deadline, collect one ack per _relevant_ seat, proceed at expiry
  regardless) and should reuse it rather than growing a parallel
  mechanism.

## Design space (not decided - options to choose between later)

**A. Client-only pacing (status quo, extended).** Clients delay sending
their _own_ next command until their local animation finishes; the server
stays exactly as blind as today. Zero protocol change. Does not solve (2)
or (3) for anyone except the acting player's own outgoing commands - it
cannot delay `BlindAuctionOpened` reaching _other_ seats, nor a bot's next
move, nor an AFK auto-play, since those are driven by server timers with
no client in the loop.

**B. Server-side flat animation budget.** The server adds a fixed,
configured delay before arming any animation-sensitive timer, guessing at
typical animation length. No new protocol messages. Brittle: wrong for
variable-length chains (a 4-deep card chain vs. a plain landing), and
diverges from whatever the client is actually doing (reduced-motion
settings, per-client frame rate).

**C. Client-acknowledged animation gate.** New `ClientMessage` (e.g.
`AnimationDone { through_seq }`) sent once a client has finished rendering
up to a point; server-side timers that care wait for the ack from the
_relevant_ seat(s) before arming, with a hard fallback timeout so a
stalled or bot seat (which never animates) can never stall the table -
same doctrine as the AFK grace (ADR-0008) and the blind-bid auto-abstain
(ADR-0018): wait, but never indefinitely.

**D. Hybrid - per-room event sequence + watermark ack (recommended
direction).** Give each `Update` a monotonic per-room sequence number.
Clients ack "rendered through seq N"; the server derives a per-room (or
per-seat, see open questions) watermark and only arms an
animation-sensitive timer once the watermark clears the relevant event.
Bot seats and any seat with no live connection auto-ack instantly (they
have no visual to wait for), so an all-bot or mixed table is never
throttled by seats that cannot animate. This directly satisfies all three
symptoms above and reuses the ADR-0018 collection-window primitive
mentioned above.

## Open questions an ADR needs to settle

1. **Scope of the pause: per-seat or table-wide?** Does _everyone_ wait
   for the discoverer's landing animation before the bid window opens
   (consistent shared pacing, but a single slow/laggy client can delay
   the whole table), or only the discoverer's own client blocks locally
   while everyone else proceeds (no single point of stall, but seats can
   see the auction open before the discoverer visually "arrives" on
   their own screen)? This is the central fairness call.
2. **Which timers are actually in scope?** `BID_WINDOW`, `afk_deadline`/
   `turn_seconds`, and `bot_think_delay()` are good candidates. The
   absolute `game_deadline` (game clock, ADR-0010) should very likely
   stay untouched - it is a hard external deadline, and repeated
   animation "stalls" (genuine or a broken/malicious client) must never
   be able to extend it indefinitely.
3. **Event grouping for multi-hop chains.** Does the engine/session layer
   need to tag which events within one `Update` form one animation
   sequence (an explicit wrapper/marker), or is it acceptable for clients
   to infer hop boundaries themselves from event _types_ (e.g. treat each
   `Moved`/`CardDrawn`/`WentToJail` as its own beat, in order)? The
   `WentToJail` `from` gap (above) needs resolving either way.
4. **Hard cap on the ack wait, independent of the timer it gates.** Same
   principle as `BID_WINDOW`'s 5s: an animation-ack window needs its own
   bounded timeout (e.g. "assume done after N seconds regardless") so a
   client that never acks (bug, malice, or a seat that disconnected
   mid-animation) cannot freeze a timer forever. This budget must not be
   attacker-controlled - a client claiming "still animating" is
   effectively a timing input from an untrusted party.
5. **Opt-out for reduced motion / spectators / the CLI.** The terminal
   client has no animations and should always auto-ack immediately - a
   useful built-in exerciser of the fallback path in tests. A future
   Flutter/web "reduced motion" setting would want the same per-seat
   auto-ack behavior; worth designing the ack contract so "I don't
   animate" and "I do animate but I'm slow" are the same code path
   (immediate ack vs. delayed ack), not two different mechanisms.
6. **Wire cost.** A new `ClientMessage` variant and a per-`Update`
   sequence number are both protocol breaks (same bar as any `Event`/
   `CommandKind` change - update web + CLI + Flutter + bot together, per
   `CLAUDE.md`).

## Worked example (Chance -> "go to jail", walked through under option D)

1. Player rolls; engine emits `Moved` (old position -> Chance tile) in
   this turn's `Update`.
2. Landing resolves the Chance tile: `CardDrawn` (the card is revealed),
   then `go_to_jail` teleports and emits `WentToJail` - all still in the
   _same_ `Update` today (one `apply()` call, ADR-0001 atomicity is about
   state mutation, not about animation pacing, so batching everything in
   one `Update` does not have to mean animating it in one beat).
3. Under option D, the session layer treats this as three animation
   beats for the acting client: (a) slide to the Chance tile, (b) pause
   for the card-reveal animation, (c) redirect/slide to Jail. The
   client acks once its local sequence of beats finishes (or per-beat, if
   finer-grained pacing turns out to matter - open question 3).
4. Whatever is gated on "this player's turn is visually done" (their own
   `EndTurn`, an AFK auto-`EndTurn`, the next `TurnStarted`) waits for
   that ack, bounded by the hard cap from open question 4.

## Non-goals of this document

Not choosing: exact message shapes, exact timeout constants, whether
per-seat or table-wide (open question 1), or an implementation order.
Once those are decided, write it up as a normal ADR (next number:
`docs/adr/0025-...`) following the existing context/decision/consequences
format, and update `docs/architecture.typ`'s frontend-doctrine paragraph
(quoted at the top of this document) to reflect the new split between
client-optimistic rendering and server-gated timers.
