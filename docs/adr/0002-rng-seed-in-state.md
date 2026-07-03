# ADR-0002: PRNG seed lives inside `GameState`

Status: accepted

## Context
The engine must be pure and replayable from a command log, yet dice and deck
shuffles are random. Randomness cannot come from ambient state (I/O ban).

## Decision
A SplitMix64 state (`u64`) is a field of `GameState`, seeded per game by the
session layer. Every draw advances it deterministically. `ClientView` omits
it (and deck order) so clients cannot predict dice or cards.

## Consequences
`(initial state, accepted command log)` replays bit-identically. No `rand`
dependency in the engine crate. Server-side seed generation is the only
entropy source.
