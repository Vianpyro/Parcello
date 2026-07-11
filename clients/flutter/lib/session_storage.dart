/// Reconnect-token persistence: `loadReconnectTokens`/`saveReconnectTokens`.
/// A file in the OS profile dir on native targets, `localStorage` on web -
/// see `session_storage_io.dart` / `session_storage_web.dart`.
library;

export 'session_storage_web.dart' if (dart.library.io) 'session_storage_io.dart';
