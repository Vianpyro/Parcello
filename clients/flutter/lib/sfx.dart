/// Sound effects (assets/sfx/). Best-effort: a missing file or an
/// unavailable audio backend is swallowed, never crashing the game.
///
/// Players are created lazily on the first sound so importing this file is
/// cheap and safe under `flutter test` (no audio platform needed there).
library;

import 'dart:math';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/widgets.dart';

class Sfx {
  /// Toggled by the mute button; when false, `_play` is a no-op.
  bool enabled = true;

  /// A small round-robin pool so overlapping sounds (a hop landing while the
  /// previous still rings) each get their own player.
  List<AudioPlayer>? _pool;
  int _next = 0;
  final _rng = Random();
  int _timerTick = 0;

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

  /// A random clip (0..9) on mouse hover over any button.
  void buttonHover() => _play('button-hover-${_rng.nextInt(10)}.mp3');

  /// A dialog confirmation was accepted (e.g. "Resign" in the resign prompt).
  void buttonYes() => _play('button-yes.mp3');

  /// A dialog confirmation was declined (e.g. "Cancel" in the resign prompt).
  void buttonNo() => _play('button-no.mp3');

  /// The server rejected a command, or reported a connection error.
  void error() => _play('error.mp3');

  void gameStart() => _play('game-start.mp3');
  void playerJoin() => _play('player-join.mp3');
  void playerLeave() => _play('player-leave.mp3');

  /// A countdown milestone (game or turn clock); cycles through the 8 clips,
  /// which carry no meaningful order.
  void timerTick() {
    _play('timer-$_timerTick.mp3');
    _timerTick = (_timerTick + 1) % 8;
  }

  void toggleOn() => _play('toggle-on.mp3');
  void toggleOff() => _play('toggle-off.mp3');
}

/// App-global sound service (audio is inherently a single shared output).
final sfx = Sfx();

/// Wraps [child] so hovering it with a mouse plays a random button-hover
/// clip. No-op on touch (no hover event fires).
Widget hoverSfx(Widget child) =>
    MouseRegion(onEnter: (_) => sfx.buttonHover(), child: child);
