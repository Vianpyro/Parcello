/// PcTextField - the design system's single-line text input (DESIGN_SYSTEM.md).
///
/// Part of the in-tree design system (DDR-0016). The DS dressing of a Material
/// `TextField`: a muted floating label, a hairline underline that turns gold
/// on focus, a gold cursor. No box fill, no shadow - the flat/hairline register
/// (ART_DIRECTION), consistent with PcCard.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): the constructor + named params
/// are public API, grown ADDITIVELY as real screens demand (never
/// speculatively). Connect (labelled form fields) shipped the first surface;
/// the Settings panel then needed dense numeric columns with an EXTERNAL label,
/// which added `keyboardType`, `textAlign`, `dense`, and loosened `label` to
/// optional - all backward-compatible (existing callers unchanged), so within
/// the DDR-0019 "add optional params freely" allowance. The in-game action bar
/// then added `inputFormatters` (the bid/bribe fields cap digits + amount as
/// you type). Still future-additive when a screen needs them: `autofocus` (the
/// join-code field), a counter toggle (feedback suppresses it).
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../tokens.dart';

/// A single-line input styled to the design system. The typed text keeps the
/// theme's default input size (comfortable to type into); only the decoration
/// is tokenised, so migrating an inline `TextField` is value-preserving.
class PcTextField extends StatelessWidget {
  final TextEditingController controller;

  /// The floating label. Omit for a field whose label sits OUTSIDE it (a
  /// settings row's left column) - then the input carries none.
  final String? label;

  /// Placeholder shown while empty; omit for none.
  final String? hint;

  /// Caps input length and shows the Material counter (e.g. the 24-char
  /// display name). Omit for an unbounded field.
  final int? maxLength;

  /// The soft-keyboard / input type - e.g. `TextInputType.number` for a
  /// numeric settings or bid field. Omit for free text.
  final TextInputType? keyboardType;

  /// Text alignment - `TextAlign.end` for a right-aligned numeric column.
  final TextAlign textAlign;

  /// Compact vertical density for a field packed into a dense list (the
  /// settings rows); the roomy default suits a standalone form field.
  final bool dense;

  /// Input formatters - e.g. digits-only + a max-value cap for a bid field, so
  /// the field itself refuses an illegal edit as you type. Omit for free text.
  final List<TextInputFormatter>? inputFormatters;

  const PcTextField({
    super.key,
    required this.controller,
    this.label,
    this.hint,
    this.maxLength,
    this.keyboardType,
    this.textAlign = TextAlign.start,
    this.dense = false,
    this.inputFormatters,
  });

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: controller,
      maxLength: maxLength,
      keyboardType: keyboardType,
      textAlign: textAlign,
      inputFormatters: inputFormatters,
      cursorColor: Pc.gold,
      decoration: InputDecoration(
        labelText: label,
        hintText: hint,
        isDense: dense,
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
