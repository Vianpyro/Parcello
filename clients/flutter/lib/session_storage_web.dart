/// Web reconnect-token persistence: `window.localStorage`. Selected for the
/// web target by the conditional export in `session_storage.dart`.
library;

import 'dart:convert';

import 'package:web/web.dart' as web;

const _key = 'parcello.reconnect';

/// Best-effort load (ADR-0008): storage errors just mean an empty starting
/// map (e.g. private browsing modes that disable localStorage).
Map<String, String> loadReconnectTokens() {
  try {
    final raw = web.window.localStorage.getItem(_key);
    if (raw == null) return {};
    final saved = jsonDecode(raw) as Map;
    return saved.cast<String, String>();
  } catch (_) {
    return {};
  }
}

/// Best-effort save: storage errors only cost the persistence, never the
/// session.
void saveReconnectTokens(Map<String, String> tokens) {
  try {
    web.window.localStorage.setItem(_key, jsonEncode(tokens));
  } catch (_) {}
}
