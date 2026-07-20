/// The C2 guard (INVARIANTS C2, DESIGN/VISUAL_LANGUAGE.md): a raw colour or
/// on-grid spacing literal at a use site is a bug - it must be a token in
/// `lib/tokens.dart`, so the visual grammar is enforceable instead of every
/// widget inventing its own values.
///
/// This scans the source rather than adding a custom analyzer plugin
/// (`custom_lint`) - zero extra dependency, runs in the normal test/CI gate.
/// It is a pragmatic guard, not a full AST lint: it uses the SAME anchored
/// patterns the spacing migration used, so by construction it reports what
/// that migration did not cover.
///
/// IN SCOPE (enforced):
///   - colours: any `Color(0x..)` outside `tokens.dart`;
///   - ON-GRID spacing (2/4/6/8/12/16/24) in simple single-value contexts:
///     `EdgeInsets.all(N)`, a single named inset side, a `SizedBox` dim;
///   - the brand font: any inline `fontFamily: 'Fraunces'` outside
///     `typography.dart` (DDR-0018 / TYPOGRAPHY.md - Fraunces is the brand
///     voice, reachable ONLY through `PcText.wordmark`; Inter is the default
///     UI face).
///
/// OUT OF SCOPE (deliberately not flagged - see `lib/tokens.dart`'s spacing
/// note and DESIGN/IMPLEMENTATION_ROADMAP.md):
///   - off-grid one-offs (3/5/7/10/18/20): no token exists for them; they
///     stay literal pending a visual-review pass that aligns them;
///   - the structural 0 and 1 (none / hairline), not spacing rhythm;
///   - multi-value bespoke insets (`fromLTRB`, mixed `symmetric`): deliberate
///     asymmetric geometry, decided per case in review, not grid spacing;
///   - `Duration` literals: an animation duration belongs in `motion.dart`,
///     but a network timeout / debounce / timer interval does not, and the
///     two cannot be told apart line-by-line - so durations are a
///     review-only C2 concern, not machine-enforced here;
///   - raw `fontSize:` values (a role SIZE like 12 has a `PcText` role):
///     deferred until the theme's default text colour is made explicit
///     (`Pc.text`) so colour-inheriting bare-size styles can adopt a
///     coloured role without a visual shift - until then a size-only style
///     that inherits its colour cannot safely take a coloured role. See
///     DESIGN/IMPLEMENTATION_ROADMAP.md Phase 2.
///
/// PROGRESSION: landed directly in ERROR mode because the spacing migration
/// reached full coverage first (the dry run reported zero violations). Set
/// [_enforce] to `false` to DOWNGRADE to warning mode during a future large
/// migration - it will print offenders without failing CI - then flip back
/// to `true` once coverage is restored.
library;

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// `true` = fail on any violation (enforced). `false` = print and pass
/// (warning mode, for use mid-migration only).
const _enforce = true;

/// The 4-px spacing grid that has tokens in `Pc` (`s2`..`s24`).
const _grid = '2|4|6|8|12|16|24';

void main() {
  test('C2 guard: no raw colour or on-grid spacing literal at a use site', () {
    final all = RegExp(r'EdgeInsets\.all\((' + _grid + r')\)');
    // Named inset sides + SizedBox dims, anchored so `4` never matches inside
    // `40`/`Pc.s4`, and off-grid values never match.
    final named = RegExp(
      r'\b(top|bottom|left|right|horizontal|vertical|height|width): '
      '($_grid)'
      r'(?=[,)\s])',
    );
    final colour = RegExp(r'Color\(0x');
    // The brand font is reachable only through PcText.wordmark.
    final fraunces = RegExp(r"fontFamily: 'Fraunces'");

    final violations = <String>[];
    for (final entity in Directory('lib').listSync(recursive: true)) {
      if (entity is! File || !entity.path.endsWith('.dart')) continue;
      // The design-system source files are the ONE place these literals are
      // allowed to live (tokens.dart: colours/spacing; typography.dart: the
      // Fraunces wordmark role). Generated l10n is not ours to police.
      if (entity.path.endsWith('tokens.dart')) continue;
      if (entity.path.endsWith('typography.dart')) continue;
      if (entity.path.contains('${Platform.pathSeparator}l10n${Platform.pathSeparator}')) {
        continue;
      }
      final lines = entity.readAsLinesSync();
      for (var i = 0; i < lines.length; i++) {
        final line = lines[i];
        if (all.hasMatch(line) ||
            named.hasMatch(line) ||
            colour.hasMatch(line) ||
            fraunces.hasMatch(line)) {
          violations.add('${entity.path}:${i + 1}  ${line.trim()}');
        }
      }
    }

    if (violations.isEmpty) return;
    final message = 'C2 guard found ${violations.length} raw literal(s) that '
        'must be a token in lib/tokens.dart '
        '(on-grid spacing -> Pc.sN; colour -> a Pc colour):\n  '
        '${violations.join('\n  ')}';
    if (_enforce) {
      fail(message);
    } else {
      // ignore: avoid_print
      print('C2 guard WARNING (not enforced):\n$message');
    }
  });
}
