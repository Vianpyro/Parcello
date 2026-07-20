/// Design tokens: the single source of truth for Parcello's palette and
/// geometry (`docs/visual-identity.md`, `DESIGN/VISUAL_LANGUAGE.md`). A
/// colour, spacing or radius exists here or it does not exist - a raw
/// literal at a use site is a bug (INVARIANTS C2), because a visual grammar
/// cannot be enforced when every widget invents its own values.
///
/// Home of the in-tree design system (DDR-0016: `lib/design/` is the target
/// folder, realized incrementally; this file is its `tokens` today).
library;

import 'package:flutter/material.dart';

/// The validated palette. Names match the `pc-*` tokens in the identity doc.
abstract final class Pc {
  // Surfaces.
  static const bg = Color(0xFF14171C);
  static const surface = Color(0xFF1E2229);
  static const surface2 = Color(0xFF262B33);
  static const border = Color(0xFF33383F);
  static const borderMuted = Color(0xFF3A4048);

  // Text: warm off-white, never pure white.
  static const text = Color(0xFFECE6D8);
  static const textMuted = Color(0xFF8C8577);
  static const textFaint = Color(0xFF655F52);

  // Paper: property faces, chits, receipts.
  static const parchment = Color(0xFFECE0C2);
  static const parchmentInk = Color(0xFF2A2420);

  // Accents.
  static const gold = Color(0xFFD8B45A);
  static const goldDark = Color(0xFFA9812F);
  static const oxblood = Color(0xFF9C433A);
  static const sage = Color(0xFF3F5240);

  /// Gain and loss, as they read on parchment (the board is light; the chrome
  /// is dark - `sage`/`oxblood` at surface weight are too dark on a tile).
  static const gainInk = Color(0xFF2F6F3E);
  static const lossInk = Color(0xFF9C433A);

  /// Sharp corners everywhere: 0-2 px. Art direction, not preference - no
  /// pills, no soft blobs (`visual-identity.md`).
  static const radius = BorderRadius.all(Radius.circular(2));

  /// The only shadow the game allows: a hairline, never a soft halo.
  static const hairShadow = [
    BoxShadow(color: Color(0x66000000), blurRadius: 2, offset: Offset(0, 1)),
  ];

  // -- Spacing scale ---------------------------------------------------------
  // The 4-px grid the UI already uses (values audited 2026-07: 12/8/6/4/2 are
  // the workhorses, 16/24 the section gaps). Reach for these instead of a raw
  // number in an `EdgeInsets` or `SizedBox` (INVARIANTS C2). Off-grid one-offs
  // (3/5/7/10/18) intentionally have NO token - they stay literals pending a
  // visual-review pass that aligns them to the grid; do not invent `s3`/`s7`.
  static const double s2 = 2;
  static const double s4 = 4;
  static const double s6 = 6;
  static const double s8 = 8;
  static const double s12 = 12;
  static const double s16 = 16;
  static const double s24 = 24;

  /// The recurring body padding of a card or panel (12 px on every side).
  /// A semantic preset over the bare `EdgeInsets.all(s12)` because "card
  /// body" is a meaning, not just a number - migrate card/panel padding to
  /// this, not every incidental 12.
  static const cardInset = EdgeInsets.all(s12);
}

/// The 8 property groups plus `utility`, in the muted tones of the identity
/// doc - deliberately NOT Monopoly brights, and deliberately far enough from
/// [pawnColors] that "whose pawn" never reads as "which group".
const groupColors = <String, Color>{
  'brown': Color(0xFF6B4A3A),
  'lightblue': Color(0xFF52708A),
  'pink': Color(0xFF8F5566),
  'orange': Color(0xFFAB6A3D),
  'red': Color(0xFF7D3D33),
  'yellow': Color(0xFFA68B3C),
  'green': Color(0xFF3F6B52),
  'navy': Color(0xFF2E3A5C),
  // Not one of the eight: the group-scaled tiles wear a gold band.
  'utility': Pc.gold,
};

/// Up to 6 players.
const pawnColors = [
  Color(0xFF4A7D7A), // teal
  Color(0xFFB5654A), // coral
  Color(0xFF7A6A9C), // violet
  Color(0xFF7F8A4A), // olive
  Color(0xFF5C728F), // slate
  Color(0xFF9C5C74), // rose
];

Color pawnColor(int seat) => pawnColors[seat % pawnColors.length];
