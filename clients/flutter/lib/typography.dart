/// Typography roles (`DESIGN/TYPOGRAPHY.md`, DDR-0018): the named text
/// styles every widget reaches for, so type is consistent instead of
/// re-specified inline. A raw `TextStyle(fontSize: ...)` at a use site is a
/// bug for the same reason a raw colour is (INVARIANTS C2) - the type
/// grammar cannot be enforced when every widget invents its own.
///
/// A role carries size + weight + family + a DEFAULT colour (its dominant
/// one). Override an atypical colour with `.copyWith(color: ...)`, which
/// then reads as intentional. Some roles deliberately OMIT `fontSize` so
/// they inherit the ambient size (a wordmark placed at an appbar/hero size,
/// a number sitting in whatever line contains it).
///
/// Home of the in-tree design system (DDR-0016: `lib/design/` is the target
/// folder, realized incrementally; this file is its `typography` today).
/// The role set GROWS as migration proceeds - add a role when a real
/// recurring combo appears, never speculatively.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): `PcText` is consumed app-wide.
/// Adding a role is free (design it to last); renaming/removing one or
/// changing its size/weight/family/default-colour semantics needs a DDR or
/// an in-diff justification, because it silently restyles every call site.
library;

import 'package:flutter/material.dart';

import 'tokens.dart';

abstract final class PcText {
  /// The brand voice: Fraunces. The wordmark and end-screen titles. Size is
  /// inherited (the wordmark sits at the appbar/hero size it is placed in).
  static const wordmark = TextStyle(
    fontFamily: 'Fraunces',
    fontWeight: FontWeight.w700,
    color: Pc.gold,
  );

  /// A prominent tile / card headline (menu tiles, big titles).
  static const tileTitle = TextStyle(
    fontSize: 18,
    fontWeight: FontWeight.w700,
    color: Pc.text,
  );

  /// A section title within a panel or screen.
  static const section = TextStyle(
    fontSize: 16,
    fontWeight: FontWeight.w700,
    color: Pc.text,
  );

  /// An emphasized row / card title.
  static const rowTitle = TextStyle(
    fontSize: 14,
    fontWeight: FontWeight.w700,
    color: Pc.text,
  );

  /// Default body text.
  static const body = TextStyle(fontSize: 13, color: Pc.text);

  /// Dense body / button label / secondary line.
  static const label = TextStyle(fontSize: 12, color: Pc.text);

  /// Caption or hint - muted by default (the dominant caption colour).
  static const caption = TextStyle(fontSize: 11, color: Pc.textMuted);

  /// Whisper: the faintest, smallest label ("unranked", tiny corner text).
  static const whisper = TextStyle(fontSize: 10, color: Pc.textFaint);

  /// Live numbers: tabular figures so a ticking value never jitters
  /// (`TYPOGRAPHY.md`: cash, timers, VP, bids). Size is inherited; compose
  /// with a size role via `.copyWith` where a specific size is wanted.
  static const amount = TextStyle(
    fontFeatures: [FontFeature.tabularFigures()],
    color: Pc.text,
  );
}
