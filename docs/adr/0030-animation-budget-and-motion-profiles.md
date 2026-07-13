# ADR-0030: client animation budget and motion profiles

Status: accepted

## Context
ADR-0028 gates the server's animation-sensitive timers on client render acks,
bounded by `ANIM_ACK_CAP` = 6s (`crates/server/src/room.rs`). Past the cap the
server proceeds regardless. It left beat durations as "starter values, tunable
client-side without protocol changes".

Tuning them turns out not to be enough: nothing bounds their *sum*. One
`Update` can chain a movement (1970ms) + a card reveal (1700ms) + a
card-driven teleport through Go (2810ms) + a salary floater (500ms) =
~6980ms, and card chains recurse to `MAX_CARD_CHAIN_DEPTH` = 4. A client
that overruns the cap is not merely slow, it is *behind the game*: the
server un-gates, the bid window opens, a bot moves - all while the client is
still animating the previous turn. That is precisely the desynchronisation
ADR-0028 exists to prevent, reintroduced from the client side.

Separately, ADR-0028 anticipated a reduced-motion setting ("the same path a
future reduced-motion setting takes" as the CLI's instant ack) but did not
specify it, and the client shipped with no accessibility knob at all.

The design work behind both is `docs/motion-language.md`; this ADR records
the two decisions in it that are contracts rather than taste.

## Decision
- **Animation budget.** `ANIM_BUDGET` = 4000ms is a client invariant: no
  `Update`'s beats may exceed it. The 2s margin under `ANIM_ACK_CAP`
  absorbs frame-rate slop and a slow first paint. The client enforces it by
  **compiling the whole Update into a plan before playing any of it** - the
  cost is known before the first frame - and compressing an over-budget plan
  in a fixed order: coalesce same-kind beats on the same subject, demote P3
  beats to their instant form, compress the exclusive lane (floor 40%), then
  truncate the middle of a chain (first and last beat always survive).
  P1 beats (bankruptcy, every win) are never compressed.
- **Tiers.** Beats are assigned one of four priorities - P1 arrest (table
  stops), P2 decide (a window is open), P3 consequence (value moved), P4
  ambient (never a beat, an implicit transition). The tier is a contract
  about *who waits*, and it is per-observer: an event that costs a seat
  something is at least one tier louder for that seat.
- **Motion profiles.** One knob, three values, honoured everywhere: Full
  (1.0), Reduced (0.5, no travel), Instant (0.0, beats apply immediately and
  the ack fires at once). Instant is not a degraded mode - it is the same
  "I do not animate" path the CLI and bot seats already take under ADR-0028,
  which is why the server needs no change to tolerate it. The platform's
  reduce-motion accessibility flag seeds the default.
- **No information may exist only in motion.** Every fact a beat conveys
  must also be readable from a static frame. This is what makes Instant a
  first-class path rather than an information loss, and it is a constraint on
  how beats are authored: `Beat.apply()` must be meaningful with zero
  duration.
- The client-side split that makes the budget enforceable (a pure
  `compile(events, view) -> Plan`, separate from execution and from the
  socket) is an internal restructuring of the Flutter client - no protocol
  change, no engine change, no deviation from `docs/architecture.typ`.

## Consequences
- The animation logic becomes unit-testable for the first time: `compile()`
  is pure, so "no plan exceeds `ANIM_BUDGET`" and "a bankruptcy coalesces"
  are assertions rather than hopes (`test/director_test.dart`).
- Long card chains (3-4 deep) are visibly truncated rather than played in
  full. Accepted: the alternative is the client falling behind the server,
  which is strictly worse, and the log retains every event.
- Beat durations stay client-side and tunable without a protocol change, as
  ADR-0028 established; only their *sum* is now bounded.
- `ANIM_BUDGET` and `ANIM_ACK_CAP` are coupled by this ADR. Changing the
  server constant without revisiting the client one reopens the desync.
- The tier system makes some events louder for the seat they hurt, so a
  compiled plan is per-seat, not a table-wide broadcast. Trade events were
  already per-seat (ADR-0007), so no new mechanism is introduced.
