/// Everything that travels *between* the board and the HUD, plus the P1 arrest.
///
/// This layer exists because of the money rule (`docs/motion-language.md` 4.2):
/// a rent payment is one object leaving the payer's pawn and landing on the
/// owner's seat marker, and those two live in different widget subtrees. A
/// floater confined to the board could never express it - which is why the old
/// client rendered only the payer's loss and left the owner, who had just
/// earned the game's core income, with nothing to see.
library;

import 'package:flutter/material.dart';

import 'motion.dart';
import 'stage.dart';
import 'tokens.dart';

class StageOverlay extends StatefulWidget {
  final StageState stage;
  const StageOverlay({super.key, required this.stage});

  @override
  State<StageOverlay> createState() => _StageOverlayState();
}

class _StageOverlayState extends State<StageOverlay> {
  final _key = GlobalKey();

  /// Anchors resolve to screen coordinates; the overlay draws in its own.
  Offset? _local(Anchor a) {
    final global = widget.stage.anchors.resolve(a);
    final box = _key.currentContext?.findRenderObject() as RenderBox?;
    if (global == null || box == null || !box.hasSize) return null;
    return box.globalToLocal(global);
  }

  /// Subscribes to the stage itself: a component handed a notifier should
  /// listen to it, rather than depend on a caller remembering to wrap it.
  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.stage,
        builder: (context, _) => _build(),
      );

  Widget _build() {
    final stage = widget.stage;
    return IgnorePointer(
      child: SizedBox.expand(
        key: _key,
        child: Stack(children: [
          for (final chit in stage.chits)
            _ChitView(
              key: ValueKey(chit.id),
              chit: chit,
              from: _local(chit.from),
              to: _local(chit.to),
              onDone: () => stage.retireChit(chit.id),
            ),
          if (stage.arrest case final a?) _ArrestView(arrest: a),
        ]),
      ),
    );
  }
}

/// A value in flight. Direction encodes valence, shape encodes category, and
/// colour is the third, redundant channel - so the ~8% of players with a
/// red-green deficiency read the sign and the direction, not the hue.
class _ChitView extends StatelessWidget {
  final Chit chit;
  final Offset? from;
  final Offset? to;
  final VoidCallback onDone;

  const _ChitView({
    super.key,
    required this.chit,
    required this.from,
    required this.to,
    required this.onDone,
  });

  Color get _ink => switch (chit.kind) {
        ChitKind.gain => Pc.gainInk,
        ChitKind.loss => Pc.lossInk,
        ChitKind.neutral => Pc.textFaint,
        // Gold that moves always means victory points. Nothing else in the game
        // is allowed to move in gold.
        ChitKind.victoryPoints => Pc.goldDark,
      };

  @override
  Widget build(BuildContext context) {
    final target = to ?? from;
    if (target == null) return const SizedBox.shrink();
    // Reduced motion drops the journey, not the information: the chit fades in
    // at its destination with the same text, sign and colour.
    final start = chit.travels ? (from ?? target) : target;

    return TweenAnimationBuilder<double>(
      tween: Tween(begin: 0, end: 1),
      duration: Motion.chit,
      curve: Motion.arrive,
      onEnd: onDone,
      builder: (context, t, child) {
        final at = Offset.lerp(start, target, t)!;
        // Rises as it goes, and fades only at the very end - the number must be
        // readable for most of its life, not for a flicker.
        final lift = chit.kind == ChitKind.victoryPoints ? -18.0 : -10.0;
        final opacity = t < 0.75 ? 1.0 : (1 - (t - 0.75) / 0.25).clamp(0.0, 1.0);
        // A boost trap sprang over this money on its way: the value grows in
        // flight, which is the causal link between "the trap fired" and "that
        // number is huge".
        final scale = chit.amplified ? 1 + 0.45 * t : 1.0;
        return Positioned(
          left: at.dx - 44,
          top: at.dy - 14 + lift * t,
          width: 88,
          child: Opacity(
            opacity: opacity,
            child: Transform.scale(scale: scale, child: child),
          ),
        );
      },
      child: Center(
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: Pc.parchment,
            borderRadius: Pc.radius,
            border: Border.all(color: _ink, width: 1.5),
            boxShadow: Pc.hairShadow,
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
            child: Text(
              chit.text,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w800,
                color: _ink,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// P1: the table stops. The board itself has already receded (the beat set the
/// flag); this is the statement laid over it. Flat colour, one Deco rule, and a
/// hold - the hold is the payload, not the motion.
class _ArrestView extends StatelessWidget {
  final Arrest arrest;
  const _ArrestView({required this.arrest});

  @override
  Widget build(BuildContext context) {
    final accent = arrest.seat == null ? Pc.gold : pawnColor(arrest.seat!);
    return Positioned.fill(
      child: TweenAnimationBuilder<double>(
        tween: Tween(begin: 0, end: 1),
        duration: Motion.establish,
        curve: Motion.inevitable,
        builder: (context, t, _) => Opacity(
          opacity: t,
          child: ColoredBox(
            color: Pc.bg.withValues(alpha: 0.72),
            child: Center(
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                // The rule sweeps in from nothing to full width.
                Container(
                    height: 2,
                    width: 420 * t,
                    color: accent),
                Container(
                  color: Pc.surface,
                  padding: const EdgeInsets.symmetric(
                      horizontal: 40, vertical: 22),
                  child: Column(mainAxisSize: MainAxisSize.min, children: [
                    Text(
                      arrest.title.toUpperCase(),
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                          fontSize: 26,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 3,
                          color: Pc.text),
                    ),
                    if (arrest.detail case final d?) ...[
                      const SizedBox(height: 6),
                      Text(d,
                          textAlign: TextAlign.center,
                          style: const TextStyle(
                              fontSize: 13, color: Pc.textMuted)),
                    ],
                  ]),
                ),
                Container(height: 2, width: 420 * t, color: accent),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}
