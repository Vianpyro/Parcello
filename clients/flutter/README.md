# Parcello Flutter client

Desktop-first (Windows) client for the Parcello server. Mirrors the embedded
web client feature-for-feature; the server remains the only authority.

```sh
# from the repo root, with a server running:
cargo run -p parcello-server -- --insecure-guest

cd clients/flutter
flutter run -d windows      # or: flutter build windows
flutter test                # protocol + widget tests
flutter analyze
```

Layout:

- `lib/protocol.dart` - Dart mirror of the `parcello-protocol` wire shapes
  (read-only projections; commands are sent as plain maps so the wire format
  stays visible at call sites) plus the event-log formatter.
- `lib/session.dart` - WebSocket connection + game state in one
  `ChangeNotifier` (the equivalent of the web client's `st` object).
- `lib/board.dart` - classic 40-tile ring on an 11x11 grid, wrap fallback
  for modded board sizes.
- `lib/oidc.dart` - OIDC Authorization Code + PKCE login against the
  identity provider (ADR-0009): system browser + loopback redirect; the
  id_token stays in memory only.
- `lib/main.dart` - login screen (guest name or account sign-in), game
  screen, per-phase action buttons, tile owner menu, trade composer.

Only dependency: `web_socket_channel`. State management is a single
`ChangeNotifier` on purpose - the whole client state is one object pushed
by the server.

When the server gains an Event or CommandKind, update `describeEvent` in
`protocol.dart` and the action buttons in `main.dart` (same drill as the
web client and the CLI).
