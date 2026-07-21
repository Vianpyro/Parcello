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
///   - corner radius: any `circular(N)` outside `tokens.dart` - the art
///     direction is sharp corners (0-2 px, `visual-identity.md`), reachable
///     ONLY through `Pc.radius`; a raw `BorderRadius.circular(4/8)` is drift;
///   - ON-GRID spacing (2/4/6/8/12/16/24) in simple single-value contexts:
///     `EdgeInsets.all(N)`, a single named inset side, a `SizedBox` dim;
///   - the brand font: any inline `fontFamily: 'Fraunces'` outside
///     `typography.dart` (DDR-0018 / TYPOGRAPHY.md - Fraunces is the brand
///     voice, reachable ONLY through `PcText.wordmark`; Inter is the default
///     UI face);
///   - a size-ONLY `TextStyle` at a role size (`TextStyle(fontSize: N)` with
///     N in 10/11/12/13/14/16/18 and no other property): it must be the
///     matching `PcText` role (whisper/caption/label/body/rowTitle/section/
///     tileTitle). Unblocked once the theme default ink became `Pc.text`
///     (A3) - a bare-size style now inherits the DS colour, so adopting a
///     coloured role is value-preserving. Multi-property styles (a weight,
///     letterSpacing, an explicit colour) and OFF-role sizes (15/20/26/...)
///     are bespoke by design and NOT flagged.
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
///   - MULTI-property or off-role `fontSize:` styles: a bespoke display size
///     (15/20/26/32), a computed size, or a size paired with a weight/colour/
///     letterSpacing a role does not carry - these are intentional one-offs,
///     decided in review, not machine-flagged. Only the size-ONLY-at-a-role-
///     size case above is enforced.
///
/// PROGRESSION: each rule lands in ERROR mode only after its migration reaches
/// full coverage (a dry run reporting zero violations) - spacing/colour first,
/// then the Fraunces rule, then the role-size rule after A3 migrated every
/// size-only style. Set [_enforce] to `false` to DOWNGRADE to warning mode
/// during a future large migration - it prints offenders without failing CI -
/// then flip back to `true` once coverage is restored.
library;

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// `true` = fail on any violation (enforced). `false` = print and pass
/// (warning mode, for use mid-migration only).
const _enforce = true;

/// The 4-px spacing grid that has tokens in `Pc` (`s2`..`s24`).
const _grid = '2|4|6|8|12|16|24';

/// The font sizes that have a `PcText` role (whisper 10, caption 11, label 12,
/// body 13, rowTitle 14, section 16, tileTitle 18). A size-ONLY style at one of
/// these must be the role.
const _roleSizes = '10|11|12|13|14|16|18';

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
    // Any raw corner radius: `circular(N)` with a digit. `Pc.radius` (the sole
    // sanctioned value) does not match; the definition in tokens.dart is
    // excluded below. Only `Radius`/`BorderRadius` use `circular(` in the tree.
    final radius = RegExp(r'circular\(\d');
    // The brand font is reachable only through PcText.wordmark.
    final fraunces = RegExp(r"fontFamily: 'Fraunces'");
    // A size-ONLY TextStyle at a role size: `fontSize: N` is the sole argument
    // (the `)` right after it), so weights/colours/other props never match.
    final roleSize = RegExp(r'TextStyle\(fontSize: (' + _roleSizes + r')\)');

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
            radius.hasMatch(line) ||
            fraunces.hasMatch(line) ||
            roleSize.hasMatch(line)) {
          violations.add('${entity.path}:${i + 1}  ${line.trim()}');
        }
      }
    }

    if (violations.isEmpty) return;
    final message = 'C2 guard found ${violations.length} raw literal(s) that '
        'must go through the design system '
        '(on-grid spacing -> Pc.sN; colour -> a Pc colour; '
        'corner radius -> Pc.radius; '
        "fontFamily: 'Fraunces' -> PcText.wordmark; size-only TextStyle at a "
        'role size -> the PcText role):\n  '
        '${violations.join('\n  ')}';
    if (_enforce) {
      fail(message);
    } else {
      // ignore: avoid_print
      print('C2 guard WARNING (not enforced):\n$message');
    }
  });

  // DDR-0020: a design-system component takes a strictly-semantic Semantic
  // Model - it must not reach into the engine/session. Skins and preview/
  // replay/spectator parity depend on this boundary, so it is guarded, not
  // just asked in the CAR. Any import of the session or the engine view types
  // (`protocol.dart`) from under `lib/design/` is a violation.
  test('DDR-0020 guard: lib/design never imports session or engine views', () {
    // Anchored to a path boundary so `session.dart`/`protocol.dart` match only
    // as a whole file (never inside a `foo_session.dart`).
    final forbidden =
        RegExp('''import\\s+['"](?:[^'"]*/)?(session|protocol)\\.dart['"]''');
    final violations = <String>[];
    final root = Directory('lib${Platform.pathSeparator}design');
    if (root.existsSync()) {
      for (final entity in root.listSync(recursive: true)) {
        if (entity is! File || !entity.path.endsWith('.dart')) continue;
        final lines = entity.readAsLinesSync();
        for (var i = 0; i < lines.length; i++) {
          if (forbidden.hasMatch(lines[i])) {
            violations.add('${entity.path}:${i + 1}  ${lines[i].trim()}');
          }
        }
      }
    }
    if (violations.isEmpty) return;
    final message =
        'DDR-0020 violated: a design-system component reaches into the engine/'
        'session (its input must be a pre-mapped, engine-free Semantic Model):'
        '\n  ${violations.join('\n  ')}';
    if (_enforce) {
      fail(message);
    } else {
      // ignore: avoid_print
      print('DDR-0020 guard WARNING (not enforced):\n$message');
    }
  });

  // Spatial blindness (the scene's composition boundary, DDR-0020 layer 2 /
  // stage.dart + overlay.dart): a component receives at most an OPAQUE anchor
  // `Key` - it never resolves a coordinate, reads another widget's RenderBox,
  // or imports the AnchorRegistry (`stage.dart`). Cross-widget positioning is
  // the stage/overlay's job, addressed by abstract anchors. Guarding this keeps
  // components previewable/skinnable and prevents component<->component spatial
  // coupling - which is why it is a source-scan, not just a CAR question.
  test('spatial-blindness guard: lib/design never resolves widget geometry',
      () {
    final reach = RegExp(
      r'findRenderObject\(|localToGlobal|globalToLocal|AnchorRegistry|'
      '''import\\s+['"](?:[^'"]*/)?stage\\.dart['"]''',
    );
    final violations = <String>[];
    final root = Directory('lib${Platform.pathSeparator}design');
    if (root.existsSync()) {
      for (final entity in root.listSync(recursive: true)) {
        if (entity is! File || !entity.path.endsWith('.dart')) continue;
        final lines = entity.readAsLinesSync();
        for (var i = 0; i < lines.length; i++) {
          if (reach.hasMatch(lines[i])) {
            violations.add('${entity.path}:${i + 1}  ${lines[i].trim()}');
          }
        }
      }
    }
    if (violations.isEmpty) return;
    final message =
        'Spatial-blindness violated: a design-system component resolves widget '
        'geometry (it may take an opaque anchor Key, but must not read positions '
        '- the stage/overlay owns cross-widget placement):'
        '\n  ${violations.join('\n  ')}';
    if (_enforce) {
      fail(message);
    } else {
      // ignore: avoid_print
      print('spatial-blindness guard WARNING (not enforced):\n$message');
    }
  });
}
