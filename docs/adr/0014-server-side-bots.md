# ADR-0014: server-side bot seats

Status: accepted (amended 2026-07: `bot::decide` now takes a caller-
provided `noise: u64` seeding its sealed-bid jitter - bots bid a random
50-200% of list price, clamped to cash, floor-respecting for the
discoverer. The randomness is injected by the session layer/CLI; the
engine stays pure given its inputs.)

## Context
Playtesting and casual play both want to fill empty seats without a second
human. A bot already exists as the CLI `--bot` autopilot (an external
process that connects as a seat), but that is a developer tool, not an
in-app feature. Players want a lobby button to add opponents, and those
bots must never lock a real player out of the room.

## Decision
The host adds/drops bots from the lobby via two new `ClientMessage`s,
`AddBot`/`RemoveBot` (host-only, lobby-only). A bot is an ordinary `Seat`
flagged `is_bot`, with a synthetic non-spoofable identity (`bot:N` /
`Bot N`, a monotonic per-room counter) and no connection (`tx: None`).

The room task drives bots itself: each loop it asks `next_bot_action()` for
the first bot with a legal move and, after an 800 ms think delay, applies
it. The decision heuristic is the **existing** CLI bot, moved into the
engine as `parcello_engine::bot::decide` so the server and CLI share one
implementation. `decide` is a pure function of `(GameContent, ClientView,
seat)` - no I/O, no rand, no clock - so hosting it in the engine keeps the
engine-purity invariant intact; it is a policy, not a rule, hence this ADR.

Bots yield to humans:
- `AddBot` is capped at `MAX_PLAYERS`, but a human joining a **full** room
  evicts the most recently added bot instead of being rejected. Only a room
  full of humans returns "room is full".
- `SeatInfo` gains `is_bot` so clients label the seat "bot" rather than
  showing it as an offline player.

Bots exist for the lobby and the game; there is no mid-game "add bot". A
disconnected human is still auto-played by the AFK timer (ADR-0008 grace),
which also acts as the safety net if a bot's smart move is ever rejected -
the game can never stall on a bot.

## Consequences
- Determinism/replay unchanged: bot moves are ordinary accepted commands in
  the same command log; `decide` reads only the public view, so the
  authoritative engine still validates every move.
- New wire surface: `add_bot` / `remove_bot` client messages and the
  `is_bot` seat flag. All three clients (web, Flutter, CLI) render bots and
  the host controls.
- The engine now carries an optional `bot` module. It stays dependency-free
  (serde + thiserror) and pure; a future WASM/mod bot would slot in behind
  the same `decide` shape.
- A table left to bots plays on its own at ~800 ms/move; a room with zero
  connected humans still dissolves after `IDLE_TIMEOUT`, so bots never keep
  an abandoned room alive.
