# ADR-0004: mod set resolved per server, not per room

Status: accepted (MVP)

## Context
The architecture allows per-room mod sets. Resolution, validation, and
distribution per room adds lifecycle states (`Starting`) and cache concerns.

## Decision
The server resolves one ordered mod list at boot (`--mod`, repeatable,
default `base`). Every room shares the resolved content; joining clients
receive the full bundle in `Joined` (mod distribution MVP). The room state
machine collapses `Starting` to a point.

## Consequences
Simple operational model (one server = one ruleset, like most Minecraft
servers). Per-room mods later = move resolution into room creation and
reintroduce `Starting`; the engine and mod layers already support it.
