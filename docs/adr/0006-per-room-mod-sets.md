# ADR-0006: per-room mod sets, without a Starting state

Status: accepted (amends ADR-0004)

## Context
ADR-0004 resolved one mod list at boot because per-room resolution seemed
to require a `Starting` room state and cache management. Community servers
want rulesets to vary per room without restarting the server.

## Decision
`ClientMessage::Create` carries an optional ordered `mods` list; omitted or
empty selects the server's boot-time default (`--mod`, unchanged). The
transport layer resolves the list synchronously at room creation (in
`spawn_blocking`; the source is small local TOML files) and hands the room
its own `ResolvedContent` - the seam `create_room(content, ...)` already
existed. Resolution failures reject the Create with an `Error` before any
room is registered, so the `Starting` state stays collapsed to a point:
by the time a room exists, its content is final.

Mod ids arrive over the wire and become path components under `--mods-dir`.
They are allowlist-validated (ASCII alphanumeric plus `-`/`_`, max 64
chars, max 16 mods) so a hostile client cannot traverse outside the mods
directory.

## Consequences
- Joining clients need no changes: the resolved bundle was already pushed
  verbatim in `Joined` (mod distribution MVP).
- The protocol change is additive; pre-0006 clients omit the field and get
  the server default.
- No caching: each Create re-reads the TOML. Fine at room-creation
  frequency; add a cache only if profiling ever says so.
- Roadmap items skipped to get here, deliberately: the asymmetric-JWT
  IdentityVerifier is blocked on a real token issuer existing, and
  Wasmtime-backed mods are blocked on the MSRV 1.75 pin (compatible
  Wasmtime releases are outdated). Both remain on the roadmap.
