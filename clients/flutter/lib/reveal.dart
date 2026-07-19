/// A one-shot 0->1 reveal whose controller ignores the platform's
/// reduced-motion flag, so the app's `MotionProfile` (ADR-0030) stays the sole
/// motion authority. `TweenAnimationBuilder` cannot do this: its internal
/// controller is `AnimationBehavior.normal`, which Flutter Web scales to 0.05x
/// whenever the browser reports reduced motion - collapsing chits, the arrest
/// and any other implicit animation to near-instant regardless of the profile.
///
/// Drop-in for a `TweenAnimationBuilder<double>(tween: Tween(begin: 0, end: 1))`
/// that plays once on mount: same `(context, t, child)` builder, `curve`,
/// `duration` and `onEnd`.
library;

import 'package:flutter/widgets.dart';

class Reveal extends StatefulWidget {
  final Duration duration;
  final Curve curve;
  final VoidCallback? onEnd;
  final Widget? child;
  final Widget Function(BuildContext context, double t, Widget? child) builder;

  const Reveal({
    super.key,
    required this.duration,
    required this.builder,
    this.curve = Curves.linear,
    this.onEnd,
    this.child,
  });

  @override
  State<Reveal> createState() => _RevealState();
}

class _RevealState extends State<Reveal> with SingleTickerProviderStateMixin {
  late final AnimationController _ctrl = AnimationController(
    vsync: this,
    duration: widget.duration,
    animationBehavior: AnimationBehavior.preserve,
  );

  @override
  void initState() {
    super.initState();
    if (widget.onEnd != null) {
      _ctrl.addStatusListener((s) {
        if (s == AnimationStatus.completed) widget.onEnd!();
      });
    }
    _ctrl.forward();
  }

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: _ctrl,
        child: widget.child,
        builder: (context, child) =>
            widget.builder(context, widget.curve.transform(_ctrl.value), child),
      );
}
