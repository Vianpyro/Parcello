# ADR-0010: time-boxed games end by net worth

Status: accepted

## Context
The design goal is fast, dynamic games (`docs/business-tour-direction.md`).
Business Tour time-boxes games and, at the buzzer, the richest player wins -
a strong "keep it short" lever. But the engine is pure and has no clock
(hard constraint): it cannot end a game on time by itself.

## Decision
Split the clock (session) from the rule (engine).

- **Engine (rule):** `GameState::net_worth(content, player)` = cash +
  property equity (full price, or price/2 when mortgaged - so mortgaging is
  net-worth neutral) + houses at build cost. `Engine::finish_on_time(state)`
  is a pure function that sets `Finished { winner }` where `winner` is the
  richest surviving player (ties break to the lowest seat) and emits
  `Event::TimeUp { winner }`. It is a no-op on an already-finished game.
- **Server (clock):** `--game-timeout <secs>` (0 = off) arms an absolute
  per-room deadline when the game starts. On expiry the room calls
  `finish_on_time`, broadcasts the resulting per-seat `Update`, and records
  the end. `GameStarted`/`Joined` carry `time_remaining` (seconds) so
  clients run a local countdown; clients also mirror the net-worth formula
  to show a live ranking.

## Replay integrity
`finish_on_time` is NOT a player command, so it is not in the accepted-command
log. It is a deterministic function of the final Active state, so replay is:
replay the accepted commands to the final Active state, then - if the game
was time-boxed and ended on time - apply `finish_on_time`. The
`same_seed_produces_identical_games` guard is unaffected (it never times
out). This extends, but does not break, the replay model of ADR-0001/0002.

## Consequences
- Engine stays pure and clock-free; the trigger lives in the session layer.
- The net-worth formula is duplicated in the clients (web, Flutter) for
  display. Keep the three in sync (a comment on each points here).
- A time-out mid-auction abandons the auction; acceptable (the game is
  over). Cash cannot change during auctions anyway, so net worth is stable.
- Per-room, host-chosen durations (like per-room mods, ADR-0006) are a
  natural follow-up; today the limit is a server-wide flag.
