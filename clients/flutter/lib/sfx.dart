/// Sound effects (assets/sfx/). Best-effort: a missing file or an
/// unavailable audio backend is swallowed, never crashing the game.
///
/// Players are created lazily on the first sound so importing this file is
/// cheap and safe under `flutter test` (no audio platform needed there).
library;

import 'dart:math';

import 'package:audioplayers/audioplayers.dart';

class Sfx {
  /// Toggled by the mute button; when false, `_play` is a no-op.
  bool enabled = true;

  /// A small round-robin pool so overlapping sounds (a hop landing while the
  /// previous still rings) each get their own player.
  List<AudioPlayer>? _pool;
  int _next = 0;
  final _rng = Random();

  List<AudioPlayer> _players() =>
      _pool ??= List.generate(6, (_) => AudioPlayer());

  Future<void> _play(String asset) async {
    if (!enabled) return;
    try {
      final pool = _players();
      final player = pool[_next];
      _next = (_next + 1) % pool.length;
      await player.play(AssetSource('sfx/$asset'));
    } catch (_) {
      // No audio backend, missing file, etc. - silence is fine.
    }
  }

  void diceRoll() => _play('dice-roll.mp3');

  /// One of the 11 movement clips, cycling by hop so a run of steps varies.
  void moveHop(int hop) {
    final n = ((hop - 1) % 11) + 1;
    _play('move-pawn-${n.toString().padLeft(2, '0')}.mp3');
  }

  /// A random landing clip (1..3).
  void pawnStop() => _play('stop-pawn-${_rng.nextInt(3) + 1}.mp3');
}

/// App-global sound service (audio is inherently a single shared output).
final sfx = Sfx();
