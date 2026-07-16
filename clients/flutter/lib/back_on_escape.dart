import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// Wraps a pushed screen so Escape pops it, and gives the route a focus
/// starting point.
///
/// Steam Input maps a controller's B button to Escape, so this is how "back"
/// works from a gamepad / Steam Deck. `Focus(autofocus)` seeds keyboard focus
/// on entry so the D-pad can then reach the AppBar back button without a mouse
/// (the back button is otherwise never selected on a fresh route).
class BackOnEscape extends StatelessWidget {
  final Widget child;
  const BackOnEscape({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () =>
            Navigator.of(context).maybePop(),
      },
      child: Focus(autofocus: true, child: child),
    );
  }
}
