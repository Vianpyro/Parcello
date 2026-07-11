/// Native reconnect-token persistence: a JSON file in the OS profile dir.
/// Selected for non-web targets by the conditional export in
/// `session_storage.dart`.
library;

import 'dart:convert';
import 'dart:io';

File _tokenFile() {
  final base =
      Platform.environment['APPDATA'] ?? Platform.environment['HOME'] ?? '.';
  return File('$base/parcello/reconnect.json');
}

/// Best-effort load (ADR-0008): IO errors just mean an empty starting map.
Map<String, String> loadReconnectTokens() {
  try {
    final saved = jsonDecode(_tokenFile().readAsStringSync()) as Map;
    return saved.cast<String, String>();
  } catch (_) {
    return {};
  }
}

/// Best-effort save: IO errors only cost the persistence, never the session.
void saveReconnectTokens(Map<String, String> tokens) {
  try {
    final file = _tokenFile();
    file.parent.createSync(recursive: true);
    file.writeAsStringSync(jsonEncode(tokens));
  } catch (_) {}
}
