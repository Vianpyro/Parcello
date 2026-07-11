# ADR-0028: animation-ack watermark (server timers wait for rendering)

Status: accepted

## Context
The server was animation-blind by design: `Engine::apply` is pure and
clockless, and every Session Layer timer (`bid_deadline`, the AFK/turn
clock, `BOT_THINK`) was anchored to wall-clock instants with no awareness
of what clients were rendering. First real playtests (2026-07) confirmed
the three symptoms `docs/animation-sync.md` had mapped in advance: the
sealed-bid window's 5s clock started the instant the movement card was
played, before players could see which property they were bidding on;
chance-card effects were invisible (nothing rendered a draw); and
card-driven relocations looked like unexplained teleports because the stop
on the card tile was never shown. That document's option D - a per-room
Update sequence number plus a client "rendered through N" ack gating the
timers - is what this ADR adopts. Decisions taken with the owner: the
pause is table-wide for the collection windows (everyone sees the landing
before the shared clock starts), the turn clock/time bank is also gated
(on the acting seat's own ack), and the full animation set ships in the
same pass (core pacing plus cash floaters and a spotlight flash).

## Decision
- `ServerMessage::Update` gains a monotonic per-room `seq`; a new
  `ClientMessage::AnimationDone { through_seq }` acks "rendered through
  N". The ack is untrusted input: clamped to what was actually sent, and
  it can only ever *release* timers earlier, never delay anything.
- The room tracks per-seat acked seqs. Bot seats and disconnected seats
  are treated as instantly settled (they render nothing); the CLI acks
  every Update immediately - the built-in exerciser of the "I don't
  animate" path, and the same path a future reduced-motion setting takes.
- A hard cap (`ANIM_ACK_CAP` = 6s from the broadcast) bounds every gate:
  a client that never acks (bug, malice, throttled background tab) delays
  a timer by at most the cap, ever - the same wait-but-never-indefinitely
  doctrine as `DISCONNECTED_GRACE` and the window auto-abstains. The
  budget is a server constant, never attacker-controlled.
- Gated timers, all recomputed in the room loop as before:
  - sealed-bid and bribe-vote windows (ADR-0018/0024): opening flags a
    gate instead of arming the 5s deadline; `refresh_gates` arms it once
    the whole table has settled (or the cap passes). Bids/votes arriving
    before the gate opens are still accepted - the engine is untouched;
    only the deadline waits.
  - the turn clock/time bank (ADR-0023): anchored to the later of the
    last accepted command and the *acting seat's own* ack, so rendering
    time never eats thinking time; the bank drain measures from the same
    anchor.
  - bot pacing (`BOT_THINK`): anchored to the table watermark, so bots
    never race ahead of what humans can see.
  - the absolute game clock (`game_deadline`, ADR-0010) is deliberately
    NOT gated: repeated animation stalls must never extend a time-boxed
    game.
- `Event::WentToJail` gains `from` (the tile the player stood on), closing
  the one event-granularity gap the audit found - clients animate the jail
  hop without tracking last-known positions themselves.
- The Flutter client gains an animation director: Updates queue and play
  strictly in order as paced beats (pawn slide per `Moved`, a card-reveal
  banner on `CardDrawn`, the jail slide, a spotlight flash, rising cash
  deltas for salary/rent/tax/card money), and the authoritative view
  applies only after the beats finish - which is also what holds the bid
  overlay and the local turn countdown back until this client has visually
  arrived. The ack goes out with the view application.

## Consequences
- Protocol break (seq + new client message + the `WentToJail` field):
  web/CLI/Flutter updated together, wire-format tests extended.
- The server's collection-window tests gained timing assertions
  (`bid_window_waits_for_animation_acks_before_its_clock_starts`,
  `bid_window_clock_starts_early_once_everyone_acks`,
  `turn_clock_waits_for_the_acting_seats_ack`); existing paused-clock
  tests pass unchanged - tokio's auto-advance simply walks through the
  cap when nobody acks.
- A room of mixed fast/slow clients paces the shared windows to its
  slowest *animating* client, bounded by the cap - the accepted cost of
  the table-wide fairness call (nobody bids on a property they have not
  seen).
- `docs/animation-sync.md` is settled by this ADR and kept as the design
  record; `docs/architecture.typ`'s frontend doctrine paragraph now
  documents the split between client-optimistic rendering and
  server-gated timers.
- Beat durations (260ms/tile hop, ~1.7s card reveal, 6s cap) are starter
  values, tunable client-side without protocol changes - calibration is a
  playtest task.
