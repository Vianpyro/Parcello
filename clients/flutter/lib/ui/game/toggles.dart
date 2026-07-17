/// The two HUD toggles: sound, and the motion profile (ADR-0030).
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../motion.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';

class MuteButton extends StatefulWidget {
  const MuteButton({super.key});

  @override
  State<MuteButton> createState() => MuteButtonState();
}

class MuteButtonState extends State<MuteButton> {
  @override
  Widget build(BuildContext context) {
    return hoverSfx(IconButton(
      iconSize: 18,
      padding: EdgeInsets.zero,
      visualDensity: VisualDensity.compact,
      constraints: const BoxConstraints(),
      tooltip: sfx.enabled
          ? AppLocalizations.of(context).muteSound
          : AppLocalizations.of(context).unmuteSound,
      icon: Icon(sfx.enabled ? Icons.volume_up : Icons.volume_off,
          color: Pc.textMuted),
      onPressed: () => setState(() => sfx.enabled = !sfx.enabled),
    ));
  }
}

/// The accessibility knob (ADR-0030): full -> reduced -> instant.
///
/// `instant` is not a degraded mode. It is the same "I do not animate" path the
/// CLI and bot seats already take under ADR-0028, which is why the server needs
/// no change to tolerate it - and why nothing in the game is ever conveyed by
/// motion alone. Pause on any frame and the game is still playable.
class MotionButton extends StatefulWidget {
  final GameSession s;
  const MotionButton({super.key, required this.s});

  @override
  State<MotionButton> createState() => MotionButtonState();
}

class MotionButtonState extends State<MotionButton> {
  static const _icons = {
    MotionProfile.full: Icons.animation,
    MotionProfile.reduced: Icons.slow_motion_video,
    MotionProfile.instant: Icons.bolt,
  };

  @override
  Widget build(BuildContext context) {
    final stage = widget.s.stage;
    return hoverSfx(IconButton(
      iconSize: 18,
      padding: EdgeInsets.zero,
      visualDensity: VisualDensity.compact,
      constraints: const BoxConstraints(),
      tooltip: 'Motion: ${stage.profile.name}',
      icon: Icon(_icons[stage.profile], color: Pc.textMuted),
      onPressed: () => setState(() {
        const cycle = MotionProfile.values;
        stage.profile = cycle[(stage.profile.index + 1) % cycle.length];
      }),
    ));
  }
}

/// The played movement card value, shown big in the middle of the board for
/// a moment after each play, then faded out (ADR-0017; like a physical board
/// game's dice result, replaced by a card since movement no longer rolls).
