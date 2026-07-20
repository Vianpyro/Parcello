/// PcCard - the design system's surface container (DESIGN_SYSTEM.md).
///
/// Part of the in-tree design system (DDR-0016). FLAT by construction: no
/// elevation, no shadow (ART_DIRECTION - the only shadow the game allows is
/// the P2 lift hairline, which is not a card). This is why it is a
/// `DecoratedBox`, not a Material `Card`: a Material Card carries a default
/// elevation shadow, which the register forbids. Migrating an existing
/// `Card(...)` to `PcCard` therefore also REMOVES its stray Material shadow -
/// a deliberate flat correction, not a value-preserving swap.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): the constructor + named params
/// are public API. Add optional params freely (defaulted); renaming/removing
/// one needs a DDR. Internals (DecoratedBox vs Material) may change.
library;

import 'package:flutter/material.dart';

import '../../tokens.dart';

/// A flat dark surface with sharp corners. Two surface levels (base and
/// raised), an optional hairline border, and built-in body padding (the
/// recurring card inset; pass `EdgeInsets.zero` for full-bleed content).
class PcCard extends StatelessWidget {
  final Widget child;

  /// The raised surface (`surface2`) instead of the base (`surface`) - use
  /// for dialogs, hover, the spectator/end-game cards that sit ABOVE the
  /// panel they are in.
  final bool raised;

  /// A 1 px hairline border (`Pc.border`) - for a card that must read as
  /// separated from an identically-coloured background.
  final bool bordered;

  /// Body padding; defaults to the standard card inset (12). `EdgeInsets.zero`
  /// for a card whose child bleeds to the edge (a full-width list).
  final EdgeInsetsGeometry padding;

  const PcCard({
    super.key,
    required this.child,
    this.raised = false,
    this.bordered = false,
    this.padding = Pc.cardInset,
  });

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: raised ? Pc.surface2 : Pc.surface,
        borderRadius: Pc.radius,
        border: bordered ? Border.all(color: Pc.border) : null,
      ),
      child: Padding(padding: padding, child: child),
    );
  }
}
