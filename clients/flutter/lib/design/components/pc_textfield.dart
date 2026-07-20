/// PcTextField - the design system's single-line text input (DESIGN_SYSTEM.md).
///
/// Part of the in-tree design system (DDR-0016). The DS dressing of a Material
/// `TextField`: a muted floating label, a hairline underline that turns gold
/// on focus, a gold cursor. No box fill, no shadow - the flat/hairline register
/// (ART_DIRECTION), consistent with PcCard.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): the constructor + named params
/// are public API. This surface is deliberately MINIMAL - exactly what the
/// first real screen (Connect) needs. The following params are KNOWN-NEEDED by
/// screens not yet migrated and will be ADDED (optional, defaulted - additive,
/// so no DDR): `keyboardType` (numeric bid/settings inputs), `autofocus` (the
/// join-code field), a counter toggle (feedback suppresses it). Each lands when
/// the screen that needs it migrates - not speculatively.
library;

import 'package:flutter/material.dart';

import '../../tokens.dart';

/// A single-line input styled to the design system. The typed text keeps the
/// theme's default input size (comfortable to type into); only the decoration
/// is tokenised, so migrating an inline `TextField` is value-preserving.
class PcTextField extends StatelessWidget {
  final TextEditingController controller;

  /// The floating label (a localized string at every real call site).
  final String label;

  /// Placeholder shown while empty; omit for none.
  final String? hint;

  /// Caps input length and shows the Material counter (e.g. the 24-char
  /// display name). Omit for an unbounded field.
  final int? maxLength;

  const PcTextField({
    super.key,
    required this.controller,
    required this.label,
    this.hint,
    this.maxLength,
  });

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: controller,
      maxLength: maxLength,
      cursorColor: Pc.gold,
      decoration: InputDecoration(
        labelText: label,
        hintText: hint,
        // Bare-colour styles: size comes from Material's decoration logic (the
        // label floats smaller, the counter is fixed) - only the colour is
        // ours, so these stay tokenised TextStyles, not PcText roles.
        labelStyle: const TextStyle(color: Pc.textMuted),
        floatingLabelStyle: const TextStyle(color: Pc.gold),
        hintStyle: const TextStyle(color: Pc.textFaint),
        counterStyle: const TextStyle(color: Pc.textFaint),
        // Focus reads through COLOUR (neutral hairline -> gold hairline), not a
        // heavier stroke - the hairline register. Both are 1 px.
        enabledBorder:
            const UnderlineInputBorder(borderSide: BorderSide(color: Pc.border)),
        focusedBorder:
            const UnderlineInputBorder(borderSide: BorderSide(color: Pc.gold)),
      ),
    );
  }
}
