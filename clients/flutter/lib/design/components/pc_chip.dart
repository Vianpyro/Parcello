/// PcChip - a small, dense, selectable chip (DESIGN_SYSTEM.md #4). A tap-toggle
/// used for tap-to-order selections: the Legal Route builder (jail) and the
/// mod picker (menu) both build an ORDERED selection by tapping chips, and a
/// selected chip reads gold. Built on `OutlinedButton` so it keeps keyboard /
/// controller focus + the gold focus ring (Steam Deck) that a bare box loses.
///
/// Part of the in-tree design system (DDR-0016). Sharp corners (ART_DIRECTION -
/// no pills). The selection ORDER badge (`#2`) is domain state the caller owns,
/// so it composes it into `label`; the chip only renders selected vs not.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): the constructor + named params
/// are public API; grow additively as real screens demand, never speculatively.
library;

import 'package:flutter/material.dart';

import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';

class PcChip extends StatelessWidget {
  /// The chip text (the caller composes any selection-order suffix, e.g.
  /// `Wi-Fi  #2`, since that ordering is its own state).
  final String label;

  /// Selected: gold fill + gold border. Unselected: a muted hairline outline.
  final bool selected;

  /// Tap handler (toggles selection at the call site); null disables the chip.
  final VoidCallback? onTap;

  const PcChip(this.label, {super.key, this.selected = false, this.onTap});

  @override
  Widget build(BuildContext context) {
    return hoverSfx(OutlinedButton(
      onPressed: onTap,
      style: OutlinedButton.styleFrom(
        minimumSize: const Size(0, 40),
        padding: const EdgeInsets.symmetric(horizontal: Pc.s12),
        shape: const RoundedRectangleBorder(borderRadius: Pc.radius),
        foregroundColor: Pc.text,
        textStyle: PcText.label,
        backgroundColor: selected ? Pc.gold.withValues(alpha: 0.3) : null,
        side: BorderSide(color: selected ? Pc.goldDark : Pc.textMuted),
      ),
      child: Text(label),
    ));
  }
}
