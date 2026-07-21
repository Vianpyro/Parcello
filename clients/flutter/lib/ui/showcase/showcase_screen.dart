/// Design Showcase - the component gallery (a "widgetbook").
///
/// A DEBUG-ONLY developer/design surface, reached from the menu only in debug
/// builds. Every design-system component gets a section here the PR it lands
/// (DESIGN/COMPONENT_INVENTORY.md): this is where "immediately reusable" is
/// proven and where visual review happens.
///
/// Exempt from l10n (INVARIANTS C1 governs PLAYER UI; this is a dev tool, like
/// test code) - plain English labels are fine here and nowhere else.
library;

import 'package:flutter/material.dart';

import '../../design/components/pc_button.dart';
import '../../design/components/pc_card.dart';
import '../../design/components/pc_chip.dart';
import '../../design/components/pc_dialog.dart';
import '../../design/components/pc_textfield.dart';
import '../../design/components/seat_tile.dart';
import '../../tokens.dart';
import '../../typography.dart';

class ShowcaseScreen extends StatelessWidget {
  const ShowcaseScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Design Showcase', style: PcText.wordmark),
        backgroundColor: Pc.surface2,
      ),
      body: ListView(
        padding: const EdgeInsets.all(Pc.s24),
        children: const [
          _ButtonsSection(),
          _CardsSection(),
          _TextFieldsSection(),
          _ChipsSection(),
          _DialogsSection(),
          _SeatTilesSection(),
          _KeyboardSection(),
          _A11yNote(),
          // Future component sections append here, in inventory order.
        ],
      ),
    );
  }
}

/// A titled block wrapping one component's demos.
class _Section extends StatelessWidget {
  final String title;
  final List<Widget> children;
  const _Section(this.title, this.children);

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: PcText.section),
        const SizedBox(height: Pc.s12),
        ...children,
        const SizedBox(height: Pc.s24),
        const Divider(color: Pc.border, height: 1),
        const SizedBox(height: Pc.s24),
      ],
    );
  }
}

/// A labelled demo row: the caption on the left, the widget on the right.
class _Demo extends StatelessWidget {
  final String label;
  final Widget child;
  const _Demo(this.label, this.child);

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: Pc.s12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 180,
            child: Text(label, style: PcText.caption),
          ),
          const SizedBox(width: Pc.s16),
          Expanded(child: child),
        ],
      ),
    );
  }
}

/// Renders [child] under an enlarged text scale, to check a component holds
/// up at high zoom (ACCESSIBILITY: panels must survive ~1.3x). Reused across
/// sections - introduced with the second component (PcCard) that needed it.
class _TextScaled extends StatelessWidget {
  final double scale;
  final Widget child;
  const _TextScaled(this.scale, this.child);

  @override
  Widget build(BuildContext context) {
    final mq = MediaQuery.of(context);
    return MediaQuery(
      data: mq.copyWith(textScaler: TextScaler.linear(scale)),
      child: child,
    );
  }
}

class _CardsSection extends StatelessWidget {
  const _CardsSection();

  @override
  Widget build(BuildContext context) {
    return _Section('PcCard', [
      const _Demo('default', PcCard(child: Text('Base surface', style: PcText.body))),
      const _Demo(
          'raised', PcCard(raised: true, child: Text('surface2', style: PcText.body))),
      const _Demo('bordered',
          PcCard(bordered: true, child: Text('hairline border', style: PcText.body))),
      const _Demo(
          'zero padding',
          PcCard(
              padding: EdgeInsets.zero,
              child: Text('full bleed', style: PcText.body))),
      // Edge case: narrow width - the card and its content must not overflow.
      const _Demo(
          'narrow (120px)',
          SizedBox(
              width: 120,
              child: PcCard(
                  child: Text('Holds at a narrow width', style: PcText.body)))),
      // Accessibility: high text zoom - content grows, the card grows with it.
      const _Demo(
          'text zoom 1.5x',
          _TextScaled(
              1.5,
              PcCard(child: Text('Scales with text', style: PcText.body)))),
    ]);
  }
}

/// PcTextField: the DS single-line input in its real states (empty with hint,
/// filled, length-capped with a counter) plus the standing edge cases (narrow
/// width, high text zoom). Stateful only because inputs need live controllers.
class _TextFieldsSection extends StatefulWidget {
  const _TextFieldsSection();

  @override
  State<_TextFieldsSection> createState() => _TextFieldsSectionState();
}

class _TextFieldsSectionState extends State<_TextFieldsSection> {
  final _empty = TextEditingController();
  final _filled = TextEditingController(text: 'ws://127.0.0.1:7878/ws');
  final _capped = TextEditingController(text: 'alice');
  final _narrow = TextEditingController();
  final _zoom = TextEditingController(text: 'scales');
  final _dense = TextEditingController(text: '25');

  @override
  void dispose() {
    for (final c in [_empty, _filled, _capped, _narrow, _zoom, _dense]) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return _Section('PcTextField', [
      _Demo('empty + hint',
          PcTextField(controller: _empty, label: 'Server URL', hint: 'ws://...')),
      _Demo('filled', PcTextField(controller: _filled, label: 'Server URL')),
      _Demo('length-capped (counter)',
          PcTextField(controller: _capped, label: 'Display name', maxLength: 24)),
      // Edge case: narrow width - label and text must ellipsize, not overflow.
      _Demo(
          'narrow (120px)',
          SizedBox(
              width: 120,
              child: PcTextField(controller: _narrow, label: 'Cash'))),
      // Accessibility: high text zoom - the field grows with its content.
      _Demo(
          'text zoom 1.5x',
          _TextScaled(
              1.5, PcTextField(controller: _zoom, label: 'Display name'))),
      // The settings pattern: dense, numeric, right-aligned, label OUTSIDE the
      // field (here the demo caption stands in for the row's left column).
      _Demo(
          'dense numeric (settings)',
          SizedBox(
              width: 84,
              child: PcTextField(
                controller: _dense,
                keyboardType: TextInputType.number,
                textAlign: TextAlign.end,
                dense: true,
              ))),
    ]);
  }
}

/// PcChip: the tap-to-order selection chip (Legal Route builder, mod picker).
/// Stateful to show the real behaviour - tap to pick (a gold chip with its
/// order badge), tap again to drop - plus a disabled chip.
class _ChipsSection extends StatefulWidget {
  const _ChipsSection();

  @override
  State<_ChipsSection> createState() => _ChipsSectionState();
}

class _ChipsSectionState extends State<_ChipsSection> {
  final List<int> _order = [];

  @override
  Widget build(BuildContext context) {
    return _Section('PcChip', [
      const _Demo('unselected / selected / disabled', SizedBox.shrink()),
      Wrap(spacing: Pc.s6, runSpacing: Pc.s6, children: [
        const PcChip('Idle'),
        PcChip('Idle', onTap: () {}),
        const PcChip('Picked  #1', selected: true),
        const PcChip('Disabled'),
      ]),
      const SizedBox(height: Pc.s12),
      const _Demo('tap to order (route / mod picker)', SizedBox.shrink()),
      Wrap(spacing: Pc.s6, runSpacing: Pc.s6, children: [
        for (final value in const [2, 3, 4, 5, 6])
          Builder(builder: (_) {
            final pos = _order.indexOf(value);
            final picked = pos >= 0;
            return PcChip(
              picked ? '$value  #${pos + 1}' : '$value',
              selected: picked,
              onTap: () => setState(() {
                picked ? _order.remove(value) : _order.add(value);
              }),
            );
          }),
      ]),
    ]);
  }
}

/// PcDialog: modal confirm/prompt, opened from a trigger button (it can only
/// be shown, not embedded). Both shapes: a two-action prompt (title + field +
/// cancel/confirm) and a single-action confirm (no cancel).
class _DialogsSection extends StatelessWidget {
  const _DialogsSection();

  @override
  Widget build(BuildContext context) {
    return _Section('PcDialog', [
      _Demo(
        'prompt (title + field + cancel/confirm)',
        PcButton('Open prompt', wide: false, onPressed: () {
          final c = TextEditingController();
          showDialog<bool>(
            context: context,
            builder: (ctx) => PcDialog(
              title: 'Sign in',
              content: PcTextField(controller: c, label: 'Identity provider'),
              cancelLabel: 'Cancel',
              primaryLabel: 'Open browser',
              onPrimary: () => Navigator.pop(ctx, true),
            ),
          ).whenComplete(c.dispose);
        }),
      ),
      _Demo(
        'single action (no cancel)',
        PcButton('Open notice', wide: false, onPressed: () {
          showDialog<void>(
            context: context,
            builder: (ctx) => PcDialog(
              title: 'Heads up',
              content: const Text('A one-way message.', style: PcText.body),
              primaryLabel: 'OK',
              onPrimary: () => Navigator.pop(ctx),
            ),
          );
        }),
      ),
      _Demo(
        'destructive (resign/leave)',
        PcButton('Open resign', wide: false,
            variant: PcButtonVariant.destructive, onPressed: () {
          showDialog<bool>(
            context: context,
            builder: (ctx) => PcDialog(
              title: 'Resign?',
              cancelLabel: 'Cancel',
              primaryLabel: 'Resign',
              destructive: true,
              onPrimary: () => Navigator.pop(ctx, true),
            ),
          );
        }),
      ),
    ]);
  }
}

/// SeatTile: the player seat row in its states - active leader, an idle seat,
/// a bot, an eliminated seat, a lobby seat before cash exists. Wrapped in a
/// PcCard because that is where it lives (the side panel). The chit anchor and
/// the sealed-bid `trailingBid` are stage-driven and omitted here.
class _SeatTilesSection extends StatelessWidget {
  const _SeatTilesSection();

  @override
  Widget build(BuildContext context) {
    return _Section('SeatTile', const [
      PcCard(
        child: Column(children: [
          SeatTile(
            seat: 0,
            name: 'Alice',
            tags: '(you)',
            active: true,
            bankrupt: false,
            rank: 1,
            cash: r'$1450',
            vpLabel: 'VP 8/20',
          ),
          SeatTile(
            seat: 1,
            name: 'Bob',
            tags: '',
            active: false,
            bankrupt: false,
            rank: 2,
            cash: r'$920',
            vpLabel: 'VP 5/20',
          ),
          SeatTile(
            seat: 2,
            name: 'Carol',
            tags: '(bot)',
            active: false,
            bankrupt: false,
            rank: 3,
            cash: r'$300',
            netWorthLabel: r'Net $410',
          ),
          SeatTile(
            seat: 3,
            name: 'Dave',
            tags: '(offline)',
            active: false,
            bankrupt: true,
            cash: r'$0',
          ),
          // Lobby seat: no cash yet, so no figures column.
          SeatTile(
            seat: 4,
            name: 'Open seat',
            tags: '',
            active: false,
            bankrupt: false,
          ),
        ]),
      ),
      // Edge case: a narrow panel - a long name must ellipsize, not overflow.
      SizedBox(
        width: 160,
        child: PcCard(
          child: SeatTile(
            seat: 5,
            name: 'A very long player name that must clip',
            tags: '(you)',
            active: true,
            bankrupt: false,
            rank: 1,
            cash: r'$12000',
          ),
        ),
      ),
    ]);
  }
}

/// Keyboard/controller: interactive components are focusable and traversable.
/// Tab (or D-pad) moves focus, Enter/Space activates; the gold focus ring is
/// visible on the focused button.
class _KeyboardSection extends StatelessWidget {
  const _KeyboardSection();

  @override
  Widget build(BuildContext context) {
    void noop() {}
    return _Section('Keyboard & focus', [
      const _Demo('tab through these ->', SizedBox.shrink()),
      FocusTraversalGroup(
        child: Row(
          children: [
            PcButton('One', onPressed: noop, wide: false),
            const SizedBox(width: Pc.s8),
            PcButton('Two', onPressed: noop, wide: false, variant: PcButtonVariant.secondary),
            const SizedBox(width: Pc.s8),
            PcButton('Three', onPressed: noop, wide: false, variant: PcButtonVariant.quiet),
          ],
        ),
      ),
    ]);
  }
}

/// A running honest note on which accessibility axes the showcase covers and
/// which are still pending (ACCESSIBILITY.md). Grows as coverage does.
class _A11yNote extends StatelessWidget {
  const _A11yNote();

  @override
  Widget build(BuildContext context) {
    return _Section('Accessibility coverage', const [
      Text(
        'Covered here: high text zoom (per-section), narrow widths, keyboard '
        'focus/traversal. Pending (ACCESSIBILITY.md): a high-contrast theme '
        '(none exists yet), reduced/instant motion (relevant once animated '
        'components land), and screen-reader Semantics (the log is the seed). '
        'Every component section should add its own edge cases as it lands.',
        style: PcText.caption,
      ),
    ]);
  }
}

class _ButtonsSection extends StatelessWidget {
  const _ButtonsSection();

  @override
  Widget build(BuildContext context) {
    void noop() {}
    return _Section('PcButton', [
      _Demo('primary', PcButton('Create', onPressed: noop)),
      _Demo('secondary',
          PcButton('Join', onPressed: noop, variant: PcButtonVariant.secondary)),
      _Demo(
          'destructive',
          PcButton('Resign',
              onPressed: noop, variant: PcButtonVariant.destructive)),
      _Demo('quiet',
          PcButton('Replay tips', onPressed: noop, variant: PcButtonVariant.quiet)),
      _Demo('with icon',
          PcButton('Watch', onPressed: noop, icon: Icons.visibility_outlined)),
      _Demo('not wide',
          PcButton('Compact', onPressed: noop, wide: false)),
      const _Demo('disabled', PcButton('Start')),
      const _Demo(
          'disabled + reason',
          PcButton('Connect',
              disabledReason: 'This server does not accept guests: sign in.')),
    ]);
  }
}
