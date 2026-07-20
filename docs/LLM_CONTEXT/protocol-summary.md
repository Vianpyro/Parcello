# Protocol summary (`crates/protocol`)

Sources of truth: crates/protocol/src/lib.rs (shapes + wire tests),
docs/INVARIANTS.md P1-P3, README "Protocol" section.

## Shape

JSON over one WebSocket at `/ws`. Externally tagged (`"type"`),
snake_case. Commands and events on the wire ARE the engine's serde
types - **the wire format is the replay format**. Auth happens once,
inside `create`/`join`/`spectate`/`queue_ranked` payloads
(`AuthPayload`: `token` XOR `guest_name`, optional `display_name`
handle, optional `reconnect` seat token).

## Client -> server

`create {auth, mods?}` | `join {code, auth}` |
`spectate {code?, auth}` (no code = server picks) | `leave` |
`start` | `play_again` | `add_bot` | `remove_bot` |
`configure {settings}` (host, lobby; clamped server-side) |
`cmd {cmd}` (engine CommandKind verbatim) |
`feedback {rating, comment?}` |
`animation_done {through_seq}` (render watermark, ADR-0028) |
`list_mods` | `queue_ranked {auth}` | `cancel_queue` |
`get_rating {auth}` | `ping`.

## Server -> client

`room_created {code}` |
`joined {code, seat, players, content, view?, reconnect?,
time_remaining?, turn_seconds?, time_bank_seconds?, settings, ranked?}` |
`spectating {code, players, content, view?, time_remaining?,
turn_seconds?, settings}` (joined's seatless mirror) |
`lobby {players, settings}` |
`game_started {view, time_remaining?, turn_seconds?, time_bank_seconds?}` |
`update {seq, events, view, banks?}` (per-seat or spectator projection) |
`rejected {error}` (offender only; error carries `code`) |
`error {message}` | `mods {ids}` | `queued {size}` |
`match_found {code}` (answer with a normal `join`) |
`rating {...}` | `ratings_updated {changes}` | `pong`.

## Evolution rules (the part that matters)

1. Changing an existing serde shape = protocol break AND replay break.
   The wire-format tests are compatibility contracts; a legitimate
   break updates them deliberately, with an ADR.
2. Evolution is additive-only: `#[serde(default,
   skip_serializing_if)]` fields, new variants. Old client + new
   server must interoperate, and vice versa. Booleans that must read
   definitively use `skip_serializing_if = "std::ops::Not::not"`
   (omit-when-false) or are always serialized (`guest_allowed`).
3. Every new `ClientMessage` variant must fail compilation until
   routed: `ws.rs::relay` matches exhaustively; connection-scoped
   variants are listed in its unreachable arm. Never add `_ =>` there.
4. `ServerMessage` is `PartialEq` but NOT `Eq` (rating payloads carry
   f64) - don't re-add `Eq`.

## Non-wire config channel

`GET /config.json` (ADR-0032): public, unauthenticated, per-deployment
client defaults (`default_issuer?`, `guest_allowed`). Additive,
omit-when-unset for optionals. Never a secrets channel.
