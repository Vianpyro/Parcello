/// PcButton - the one button of the design system (DESIGN_SYSTEM.md).
///
/// Part of the in-tree design system (DDR-0016: `lib/design/` is the target
/// folder, realized incrementally - new components land here while the older
/// `tokens`/`typography`/`motion` files move down later).
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): the constructor and its named
/// params are public API, grown ADDITIVELY as real screens demand. Connect/
/// lobby shipped `wide`; the in-game action bar then added `dense` (compact,
/// touch-sized, intrinsic width) - a defaulted param, so old call sites are
/// untouched. Renaming/removing one, or changing a variant's meaning, needs a
/// DDR. Internals (which Material button backs each variant, the exact
/// ButtonStyle) may change freely.
library;

import 'package:flutter/material.dart';

import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';

/// The four button roles. One primary per view (the obvious next step);
/// everything else is quieter (DESIGN_SYSTEM.md).
enum PcButtonVariant {
  /// Filled gold - the single primary action of a view state.
  primary,

  /// Gold outline on dark - secondary actions.
  secondary,

  /// Oxblood - destructive / aggressive (Resign, bankruptcy-adjacent).
  destructive,

  /// Text-only, muted - tertiary ("replay tips", cancel).
  quiet,
}

/// A button in the Parcello register: sharp corners, flat colour, a hover
/// earcon, and - when disabled with a reason - the greyed-with-explanation
/// pattern (a dead button never sits unexplained, DESIGN_SYSTEM.md).
class PcButton extends StatelessWidget {
  /// The (already-localized) label. Never a raw literal from a widget - the
  /// caller passes an ARB string.
  final String label;

  /// `null` disables the button. When [disabledReason] is also set, the
  /// reason is shown beneath it.
  final VoidCallback? onPressed;

  final PcButtonVariant variant;

  /// Optional leading icon.
  final IconData? icon;

  /// Full-width and tall (the touch-friendly default), vs. intrinsic width.
  final bool wide;

  /// Compact: intrinsic width, a shorter but still touch-sized height, tighter
  /// padding - for a bar of many buttons (the in-game action bar). Overrides
  /// [wide].
  final bool dense;

  /// Shown as a caption under the button while it is disabled - the
  /// "why can't I press this?" answer (e.g. guests-off servers).
  final String? disabledReason;

  const PcButton(
    this.label, {
    super.key,
    this.onPressed,
    this.variant = PcButtonVariant.primary,
    this.icon,
    this.wide = true,
    this.dense = false,
    this.disabledReason,
  });

  @override
  Widget build(BuildContext context) {
    // The label's type identity: 16/w600. Bespoke to the button (there is no
    // exact PcText role); a future `PcText.button` role would replace this.
    final textStyle = WidgetStateProperty.all(
      const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
    );
    // dense: a compact but still touch-sized (44) button, intrinsic width;
    // wide: full-width and 52 tall; neither: intrinsic, theme height.
    final minSize = WidgetStateProperty.all<Size?>(
      dense
          ? const Size(0, 44)
          : (wide ? const Size.fromHeight(52) : null),
    );
    final padding = dense
        ? WidgetStateProperty.all<EdgeInsetsGeometry?>(
            const EdgeInsets.symmetric(horizontal: Pc.s16))
        : null;
    final child = icon == null
        ? Text(label)
        : Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 18),
              const SizedBox(width: Pc.s8),
              Text(label),
            ],
          );

    final Widget button = switch (variant) {
      PcButtonVariant.primary => FilledButton(
          onPressed: onPressed,
          style: ButtonStyle(
              minimumSize: minSize, textStyle: textStyle, padding: padding),
          child: child,
        ),
      PcButtonVariant.secondary => OutlinedButton(
          onPressed: onPressed,
          style: ButtonStyle(
              minimumSize: minSize, textStyle: textStyle, padding: padding),
          child: child,
        ),
      PcButtonVariant.destructive => OutlinedButton(
          onPressed: onPressed,
          style: ButtonStyle(
            minimumSize: minSize,
            textStyle: textStyle,
            padding: padding,
            foregroundColor: WidgetStateProperty.all(Pc.oxblood),
            side: WidgetStateProperty.all(
              const BorderSide(color: Pc.oxblood),
            ),
          ),
          child: child,
        ),
      PcButtonVariant.quiet => TextButton(
          onPressed: onPressed,
          style: ButtonStyle(
            minimumSize: minSize,
            textStyle: textStyle,
            padding: padding,
            foregroundColor: WidgetStateProperty.all(Pc.textMuted),
          ),
          child: child,
        ),
    };

    final withHover = hoverSfx(button);
    final reason = disabledReason;
    if (onPressed != null || reason == null) return withHover;

    // Disabled + a reason: show the button and, beneath it, why.
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        withHover,
        const SizedBox(height: Pc.s4),
        Text(reason, textAlign: TextAlign.center, style: PcText.caption),
      ],
    );
  }
}
