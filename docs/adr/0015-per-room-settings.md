# ADR-0015: per-room, host-editable game settings

Status: accepted

## Context
The server already hosts many rooms at once (one Tokio task each), and each
room resolves its own mod content (ADR-0006). But the two time limits
(`--turn-timeout`, `--game-timeout`) were server-wide: every room inherited
the operator's single value. And the rule scalars (`RuleParams`: economy,
auctions, expropriation, rent boosts, domination win) could only be changed
by authoring a mod. A host could not tune a single game.

The goal: the host picks all the options we already offer, per game, in the
lobby - without a server orchestrator, since one server already runs many
independent games.

## Decision
A room carries a `RoomSettings { game_seconds, turn_seconds, rules }`
(protocol type), initialised at creation from the room's mod rules and the
server's default timers. The host edits it live in the lobby via a new
`ClientMessage::Configure { settings }` (host-only, lobby-only). Every
`Lobby`/`Joined` message carries the current settings so joiners see them and
the host's edits propagate; the two clients render an editable panel for the
host and a read-only view for everyone else.

Applied at `start_game`: the engine is **rebuilt** with the effective content
(the mod's board/cards plus `settings.rules`), so `new_game` deals the chosen
economy and every command is validated against the chosen rules. The turn
timer and game clock are derived from `settings.turn_seconds` /
`settings.game_seconds`. Settings freeze once the game is Active
(`Configure` is rejected outside the lobby), preserving determinism: the
accepted-command log plus the seed and the applied rules still replays
bit-identically.

Defaults changed: new rooms are time-boxed at **60 minutes** with a **25 s**
turn limit (a strict auto-skip of the acting player), matching the
fast/dynamic design goal. `--turn-timeout` / `--game-timeout` now set the
per-room *default* (0 = disabled); the host overrides them per room.

**Untrusted input**: the wire settings are host-supplied, so the server
`clamp`s every field to a safe range before applying it (e.g. house cap
1..=5 so rent lookups stay in bounds, timers with a floor, non-negative
economy). The clamped result is broadcast back so all clients converge on
what the server accepted.

## Consequences
- Multi-game with independent settings needs no orchestrator: it was already
  one-task-per-room; only the settings moved from server-wide to per-room.
- New wire surface: `configure` message, `RoomSettings` on `Joined`/`Lobby`.
  All three clients handle it (web + Flutter editable panel; CLI `set` /
  `settings` print).
- `start_game` can now fail (the engine rebuild returns `Result`); in
  practice rule scalars never break structural validation, but the host gets
  an error instead of a panic if they ever did.
- The server-side bot and the CLI `--bot` read the effective rules (the
  rebuilt engine's content / the patched Joined content), so they play by the
  room's actual settings (ADR-0014).
- The engine stays pure and rule-agnostic; all clamping/policy lives in the
  session layer.
