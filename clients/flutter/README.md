# Parcello Flutter client

One codebase for desktop (Windows, Linux, macOS) and the browser (ADR-0025)
- the server serves the web build itself and remains the only authority.

```sh
# from the repo root, with a server running:
cargo run -p parcello-server -- --insecure-guest

cd clients/flutter
flutter run -d windows      # or: flutter build windows/linux/macos
flutter run -d chrome       # or: flutter build web --release
flutter test                # protocol + oidc tests
flutter analyze
```

Layout:

- `lib/protocol.dart` - Dart mirror of the `parcello-protocol` wire shapes
  (read-only projections; commands are sent as plain maps so the wire format
  stays visible at call sites) plus the event-log formatter.
- `lib/session.dart` - WebSocket connection + game state in one
  `ChangeNotifier` (the equivalent of the web client's `st` object).
- `lib/board.dart` - any `4*(d-1)` square ring (32 -> 9x9, 40 -> 11x11),
  wrap fallback for other board sizes. Tile text scales with cell size for
  legibility; pawns ride an animated overlay that hops square by square
  (eased per tile, ~260ms each) on a normal move and slides straight for
  teleports/jail.
- `lib/oidc.dart` - OIDC Authorization Code + PKCE login against the
  identity provider (ADR-0009); the id_token stays in memory only. A
  compile-time conditional export (`dart.library.js_interop`) picks the
  real implementation per target: `oidc_login_io.dart` (system browser +
  loopback redirect, native only - `dart:io` doesn't exist on web) or
  `oidc_login_web.dart` (popup + `postMessage`, catches the redirect via
  `web/oidc-callback.html`). Both share PKCE/JWT/discovery helpers from
  `oidc_common.dart`, so the public API (`loginWithOidc`, `pkceChallenge`,
  `jwtDisplayName`) is identical everywhere.
- `lib/lan_discovery.dart` / `lib/server_manager.dart` - same
  conditional-export pattern for the two desktop-only features with no
  browser equivalent (UDP multicast discovery, local server process
  control): the native implementation (`_io.dart`) on desktop, a
  never-reached stub (`_stub.dart`) on web, where `main.dart` hides their
  menu entries behind `kIsWeb`.
- `lib/session_storage.dart` - same pattern again for reconnect-token
  persistence (ADR-0008): a JSON file in the OS profile dir on desktop,
  `window.localStorage` on web.
- `lib/main.dart` - three screens: **Connect** (server URL + identity, the
  socket stays open), **Menu** (create a private game, join by code, public
  games "coming soon" - desktop only), and the **Game** screen (per-phase
  action buttons, tile owner menu, trade composer, movement card flash,
  play-again/continue). Buttons are full-width and >=46px tall so a mobile
  port needs little rework (`wideButton` helper).

`lib/sfx.dart` plays the sound effects in `assets/sfx/` (via `audioplayers`,
best-effort/defensive): `dice-roll` on a movement card play, a `move-pawn-NN` per tile as
the pawn hops, a random `stop-pawn` on landing; a mute button toggles
`sfx.enabled`. Runtime dependencies beyond Flutter itself: `web_socket_channel`,
`http` and `web` (OIDC), `crypto` (PKCE), `audioplayers`.
State management is a single `ChangeNotifier` on purpose - the whole client
state is one object pushed by the server.

When the server gains an Event or CommandKind, update `describeEvent` in
`protocol.dart` and the action buttons in `main.dart` (same drill as the
CLI).
