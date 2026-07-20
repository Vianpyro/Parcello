/// PcDialog - the design system's confirm/prompt dialog (DESIGN_SYSTEM.md).
///
/// Part of the in-tree design system (DDR-0016). A title, an optional body, and
/// a primary action with an optional quiet Cancel - the DS dressing of a
/// Material `AlertDialog`: the RAISED surface (`surface2`, dialogs sit above the
/// screen), a `PcText.section` title, and `PcButton` actions. Used inside
/// `showDialog`.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): the constructor + named params
/// are public API. Deliberately MINIMAL - what the first real screen (Connect's
/// sign-in) needs. Known-needed but NOT yet added (additive, defaulted, so no
/// DDR when they land with the screen that needs them): `destructive` (the
/// resign confirm - primary becomes a destructive PcButton). Added when that
/// screen migrates, not speculatively.
library;

import 'package:flutter/material.dart';

import '../../tokens.dart';
import '../../typography.dart';
import 'pc_button.dart';

/// A modal confirm/prompt. `onPrimary` decides what the dialog returns (the
/// caller pops with a value); Cancel, when present, simply dismisses.
class PcDialog extends StatelessWidget {
  /// The dialog heading (a localized string at every real call site).
  final String title;

  /// Optional body: a prompt field, a message. Omit for a bare confirm.
  final Widget? content;

  /// The confirming action's label.
  final String primaryLabel;

  /// The confirming action. The caller owns the result, e.g.
  /// `() => Navigator.pop(context, true)`.
  final VoidCallback onPrimary;

  /// A quiet dismiss action; omit for a single-action dialog. Pops with no
  /// result (a null return, which confirm flows read as "not confirmed").
  final String? cancelLabel;

  const PcDialog({
    super.key,
    required this.title,
    required this.primaryLabel,
    required this.onPrimary,
    this.content,
    this.cancelLabel,
  });

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: Pc.surface2,
      title: Text(title, style: PcText.section),
      content: content,
      actions: [
        if (cancelLabel != null)
          PcButton(
            cancelLabel!,
            onPressed: () => Navigator.pop(context),
            variant: PcButtonVariant.quiet,
            wide: false,
          ),
        PcButton(primaryLabel, onPressed: onPrimary, wide: false),
      ],
    );
  }
}
