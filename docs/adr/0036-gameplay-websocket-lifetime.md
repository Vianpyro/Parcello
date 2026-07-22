# ADR-0036: Lifetime of the gameplay WebSocket

Status: Accepted (temporary - see "Revisit").

## Context

The Flutter client opens its gameplay WebSocket as soon as the player
connects to a server, in `Session.connect` (`session.dart`). The socket
becoming ready is what reveals the main menu at all: `main.dart` routes
`joined -> GameScreen`, else `connected -> MenuScreen`, else
`ConnectScreen`. So today a player sitting in the menu already holds an open
socket.

An audit of what that socket does *before* a room is entered found almost
nothing:

- `list_mods` (the create-room mod picker) is the only menu-resident
  consumer, and it is only sent when the player expands the "Modded" option -
  not on menu entry.
- `create` / `join` / `spectate` are room-entry transitions, not
  menu-resident features; the socket only matters at the instant of the click.
- Ranked (`queue_ranked`, `get_rating`, ...) is defined server-side
  (ADR-0034) but not yet wired in the client; the ranked menu is greyed.
- Server reachability and `guest_allowed` come from a plain HTTP
  `GET /config.json` probe (ADR-0032), independent of the socket.
- The client sends no periodic ping of its own; the menu socket is passive.

So no feature *requires* the socket to be open while the player is in the
menu. The socket is idle plumbing there, and idle upgraded connections are
exactly what a reverse proxy (Nginx Proxy Manager defaults to a 60s read
timeout) drops - which surfaced as menu players being bounced back to the
connect screen.

## Decision

Keep the current lifetime: the client opens the socket at connect time and
holds it across the menu. To make that safe behind proxies, the server keeps
the connection warm with native WebSocket Ping frames after ~25s of silence
in either direction (the writer task in `crates/server/src/ws.rs`; browsers
and the Dart VM answer the Ping with a Pong at the protocol layer, so it is
invisible to the game protocol). This is a transport concern only - no
change to `ClientMessage`/`ServerMessage` or any game message.

The alternative - opening the socket lazily only on Join/Create/Spectate and
closing it on return to the menu, with the menu backed by the existing HTTP
probe - was evaluated and is deferred, not adopted.

## Consequences

- Simplicity: one connection lifecycle, no lazy-open/close dance in the menu
  transitions, and no risk of a first-click latency hit on Join/Create.
- The heartbeat already removes the proxy-idle-timeout failure that was the
  main practical cost of holding the socket open. A dropped menu socket no
  longer boots the player to the connect screen, because it no longer drops.
- Accepted costs, tracked here so they are not forgotten: each menu-only
  player still occupies a slot of the `MAX_CONNECTIONS` = 1024 semaphore plus
  a writer task, and the heartbeat spends keepalive traffic on connections
  that are doing nothing until a room is entered. On a community-hosted
  server a crowd of idle lobby sockets competes with real games for capacity.
- Forward-compatible with the roadmap: ranked matchmaking (ADR-0034) queues
  from the menu, before any room exists, and any future menu-level push
  (notifications, invites) would want a live socket there too. Keeping the
  socket open now means those land without re-architecting the menu's
  connection model.

## Alternatives considered

Lazy socket, room-scoped lifetime: show the menu on "server reachable" (the
HTTP probe) rather than on `connected`; open the socket inside
`create/join/spectate`; on return to the menu send `leave` then close the
socket. Serve the mod list over HTTP (or open the socket when the "Modded"
picker expands). Auth and reconnect tokens are unaffected - they are
persisted and sent in `_auth(code)` at join time regardless.

Rejected for now because the user-visible gain is small today (the heartbeat
already fixes the proxy timeout), it complicates the menu <-> room
transitions, and ranked will likely need a pre-room socket anyway - which
would partly undo the lazy model.

## Revisit

- When ranked matchmaking is wired into the client: confirm whether the
  pre-room socket it needs is best served by this always-open model or by a
  lazily-opened one scoped to "queueing + in a room".
- If the menu stays fully passive (no ranked, no notifications) for long
  while idle-lobby connection pressure on community servers becomes real:
  the lazy, room-scoped lifetime above is the ready fallback.
