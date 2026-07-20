/// The HUD's ticking clocks.
library;

import 'dart:async';

import 'package:flutter/material.dart';

import '../../sfx.dart';
import '../../tokens.dart';

/// Ticking countdown to a deadline. Used for the timed-game clock
/// (ADR-0010), the per-turn AFK timer, and the personal time bank
/// (ADR-0023); turns red under `warnSecs`.
class Countdown extends StatefulWidget {
  final DateTime endsAt;
  final IconData icon;
  final int warnSecs;
  /// While now is before `holdUntil`, the displayed value freezes at
  /// `endsAt - holdUntil` instead of ticking down from `endsAt - now` - the
  /// personal time bank must read as a flat reserve for the whole plain
  /// turn window and only start draining once that window is spent
  /// (ADR-0023), not from the moment the turn begins.
  final DateTime? holdUntil;
  /// While true, freezes the display at whatever it last showed instead of
  /// ticking down (ADR-0028): none of these server timers are actually
  /// running while the table is still rendering an Update, so the display
  /// must not look like it is - a fresh deadline always follows once the
  /// animation settles, at which point this naturally shows the full
  /// duration again rather than jumping.
  final bool paused;
  const Countdown(
      {super.key,
      required this.endsAt,
      this.icon = Icons.timer,
      this.warnSecs = 60,
      this.holdUntil,
      this.paused = false});

  @override
  State<Countdown> createState() => CountdownState();
}

class CountdownState extends State<Countdown> {
  // Seconds-remaining values worth a countdown cue: the final stretch plus
  // the "heads up" marks further out.
  static const _milestones = {60, 30, 10, 5, 4, 3, 2, 1, 0};

  Timer? _timer;
  int? _lastTicked;
  int? _frozenSecs;

  int _secsLeft() {
    if (widget.paused) return _frozenSecs ?? _liveSecsLeft();
    return _frozenSecs = _liveSecsLeft();
  }

  int _liveSecsLeft() {
    final now = DateTime.now();
    final holdUntil = widget.holdUntil;
    final reference =
        (holdUntil != null && now.isBefore(holdUntil)) ? holdUntil : now;
    final left = widget.endsAt.difference(reference);
    return left.isNegative ? 0 : left.inSeconds;
  }

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (widget.paused) return; // no tick cue while frozen
      final secs = _secsLeft();
      if (secs != _lastTicked && _milestones.contains(secs)) {
        _lastTicked = secs;
        sfx.timerTick();
      }
      setState(() {});
    });
  }

  @override
  void didUpdateWidget(covariant Countdown oldWidget) {
    super.didUpdateWidget(oldWidget);
    // A new deadline (next turn, restarted game clock) resets the cues.
    if (oldWidget.endsAt != widget.endsAt) {
      _lastTicked = null;
      _frozenSecs = null;
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final secs = _secsLeft();
    final mmss =
        '${(secs ~/ 60).toString().padLeft(2, '0')}:${(secs % 60).toString().padLeft(2, '0')}';
    final warn = secs <= widget.warnSecs;
    final color =
        warn ? Pc.oxblood : Pc.text;
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Icon(widget.icon, size: 18, color: color),
      const SizedBox(width: Pc.s4),
      Text(mmss,
          style: TextStyle(
            fontWeight: FontWeight.bold,
            fontFeatures: const [FontFeature.tabularFigures()],
            color: color,
          )),
    ]);
  }
}

/// Toggles sound effects on/off (`sfx.enabled`).
