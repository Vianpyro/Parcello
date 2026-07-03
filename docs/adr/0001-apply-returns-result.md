# ADR-0001: `Engine::apply` returns `Result`, not a bare tuple

Status: accepted

## Context
The architecture document specifies `fn apply(state, cmd) -> (GameState, Vec<Event>)`.
Invalid commands (out of turn, wrong phase, insufficient funds) need a
representation. Encoding them as events would force every consumer to scan
the event list to learn whether the state advanced.

## Decision
`apply(&GameState, &PlayerCommand) -> Result<(GameState, Vec<Event>), CommandError>`.
Rejections are typed, serializable, never mutate state, and are forwarded to
the issuing player only. Accepted commands keep the documented shape.

## Consequences
The command log contains accepted commands only, which is exactly the replay
input. Clients get precise rejection codes for UX.
