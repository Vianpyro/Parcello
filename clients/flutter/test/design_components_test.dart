/// Widget tests for the design-system components + the Design Showcase.
/// Each component lands with its test here (DESIGN/COMPONENT_INVENTORY.md).
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/design/components/pc_button.dart';
import 'package:parcello_client/design/components/pc_card.dart';
import 'package:parcello_client/design/components/pc_chip.dart';
import 'package:parcello_client/design/components/pc_dialog.dart';
import 'package:parcello_client/design/components/pc_textfield.dart';
import 'package:parcello_client/design/components/seat_tile.dart';
import 'package:parcello_client/tokens.dart';
import 'package:parcello_client/ui/showcase/showcase_screen.dart';

Widget _host(Widget child) =>
    MaterialApp(home: Scaffold(body: Center(child: child)));

void main() {
  group('PcButton', () {
    testWidgets('renders its label and fires onPressed', (tester) async {
      var taps = 0;
      await tester.pumpWidget(_host(PcButton('Go', onPressed: () => taps++)));
      expect(find.text('Go'), findsOneWidget);
      await tester.tap(find.text('Go'));
      expect(taps, 1);
    });

    testWidgets('is disabled when onPressed is null', (tester) async {
      await tester.pumpWidget(_host(const PcButton('Start')));
      final button = tester.widget<ButtonStyleButton>(
          find.byType(FilledButton));
      expect(button.onPressed, isNull, reason: 'null onPressed => disabled');
    });

    testWidgets('shows the reason under a disabled button', (tester) async {
      await tester.pumpWidget(_host(
          const PcButton('Connect', disabledReason: 'guests are off')));
      expect(find.text('Connect'), findsOneWidget);
      expect(find.text('guests are off'), findsOneWidget,
          reason: 'a disabled button never sits unexplained');
    });

    testWidgets('each variant renders', (tester) async {
      for (final v in PcButtonVariant.values) {
        await tester.pumpWidget(_host(PcButton('X', onPressed: () {}, variant: v)));
        expect(find.text('X'), findsOneWidget, reason: 'variant $v renders');
      }
    });

    testWidgets('dense stays touch-sized but intrinsic width', (tester) async {
      await tester.pumpWidget(_host(PcButton('Bid', onPressed: () {}, dense: true)));
      final size = tester.getSize(find.byType(FilledButton));
      expect(size.height, greaterThanOrEqualTo(44),
          reason: 'a dense button keeps a touch-friendly height');
      expect(size.width, lessThan(300),
          reason: 'dense is intrinsic width, not full-width');
    });
  });

  group('PcCard', () {
    testWidgets('renders its child', (tester) async {
      await tester.pumpWidget(_host(const PcCard(child: Text('body'))));
      expect(find.text('body'), findsOneWidget);
    });

    testWidgets('raised uses surface2, base uses surface', (tester) async {
      await tester.pumpWidget(_host(const PcCard(raised: true, child: Text('x'))));
      final box = tester.widget<DecoratedBox>(find.descendant(
          of: find.byType(PcCard), matching: find.byType(DecoratedBox)));
      expect((box.decoration as BoxDecoration).color, Pc.surface2);
    });

    testWidgets('bordered draws a hairline border', (tester) async {
      await tester.pumpWidget(_host(const PcCard(bordered: true, child: Text('x'))));
      final box = tester.widget<DecoratedBox>(find.descendant(
          of: find.byType(PcCard), matching: find.byType(DecoratedBox)));
      expect((box.decoration as BoxDecoration).border, isNotNull);
    });

    testWidgets('holds a child at a very narrow width without overflow',
        (tester) async {
      await tester.pumpWidget(_host(const SizedBox(
          width: 90,
          child: PcCard(child: Text('Holds at a narrow width')))));
      // A pumped overflow throws during layout; reaching here means no overflow.
      expect(find.byType(PcCard), findsOneWidget);
    });
  });

  group('PcTextField', () {
    testWidgets('renders its label and reflects typing', (tester) async {
      final c = TextEditingController();
      addTearDown(c.dispose);
      await tester.pumpWidget(
          _host(PcTextField(controller: c, label: 'Server URL')));
      expect(find.text('Server URL'), findsOneWidget);
      await tester.enterText(find.byType(PcTextField), 'ws://host');
      expect(c.text, 'ws://host');
    });

    testWidgets('maxLength shows a counter and caps input', (tester) async {
      final c = TextEditingController();
      addTearDown(c.dispose);
      await tester.pumpWidget(_host(
          PcTextField(controller: c, label: 'Display name', maxLength: 5)));
      await tester.enterText(find.byType(PcTextField), 'abcdefghij');
      expect(c.text, 'abcde', reason: 'input is capped at maxLength');
      expect(find.textContaining('/5'), findsOneWidget,
          reason: 'the length counter shows');
    });

    testWidgets('forwards inputFormatters (digits-only, the bid field path)',
        (tester) async {
      final c = TextEditingController();
      addTearDown(c.dispose);
      await tester.pumpWidget(_host(PcTextField(
        controller: c,
        keyboardType: TextInputType.number,
        dense: true,
        inputFormatters: [FilteringTextInputFormatter.digitsOnly],
      )));
      await tester.enterText(find.byType(PcTextField), '9a9b');
      expect(c.text, '99', reason: 'non-digits are filtered as you type');
    });

    testWidgets('shows a hint while empty', (tester) async {
      final c = TextEditingController();
      addTearDown(c.dispose);
      await tester.pumpWidget(_host(
          PcTextField(controller: c, label: 'Server URL', hint: 'ws://...')));
      expect(find.text('ws://...'), findsOneWidget);
    });

    testWidgets('holds up at a very narrow width without overflow',
        (tester) async {
      final c = TextEditingController();
      addTearDown(c.dispose);
      await tester.pumpWidget(_host(SizedBox(
          width: 90, child: PcTextField(controller: c, label: 'Cash'))));
      expect(find.byType(PcTextField), findsOneWidget);
    });

    testWidgets('label-less dense numeric field carries no label and accepts '
        'numbers (the settings pattern)', (tester) async {
      final c = TextEditingController();
      addTearDown(c.dispose);
      await tester.pumpWidget(_host(SizedBox(
          width: 84,
          child: PcTextField(
            controller: c,
            keyboardType: TextInputType.number,
            textAlign: TextAlign.end,
            dense: true,
          ))));
      final field = tester.widget<TextField>(find.byType(TextField));
      expect(field.decoration?.labelText, isNull, reason: 'no floating label');
      expect(field.keyboardType, TextInputType.number);
      expect(field.textAlign, TextAlign.end);
      await tester.enterText(find.byType(PcTextField), '3600');
      expect(c.text, '3600');
    });
  });

  group('PcDialog', () {
    // Opens a PcDialog from a button and returns the showDialog future's value,
    // so a test can assert what confirm/cancel resolved to.
    Future<bool?> openAndReturn(
        WidgetTester tester, PcDialog Function(BuildContext ctx) build) async {
      bool? result;
      await tester.pumpWidget(_host(Builder(builder: (context) {
        return PcButton('open', onPressed: () async {
          result = await showDialog<bool>(
              context: context, builder: (ctx) => build(ctx));
        });
      })));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      return result;
    }

    testWidgets('shows title, content and both actions', (tester) async {
      await openAndReturn(
          tester,
          (ctx) => PcDialog(
                title: 'Sign in',
                content: const Text('body'),
                cancelLabel: 'Cancel',
                primaryLabel: 'Open browser',
                onPrimary: () => Navigator.pop(ctx, true),
              ));
      expect(find.text('Sign in'), findsOneWidget);
      expect(find.text('body'), findsOneWidget);
      expect(find.text('Cancel'), findsOneWidget);
      expect(find.text('Open browser'), findsOneWidget);
    });

    testWidgets('primary fires onPrimary and confirms', (tester) async {
      late Future<bool?> pending;
      await tester.pumpWidget(_host(Builder(builder: (context) {
        return PcButton('open', onPressed: () {
          pending = showDialog<bool>(
            context: context,
            builder: (ctx) => PcDialog(
              title: 'T',
              primaryLabel: 'Yes',
              cancelLabel: 'No',
              onPrimary: () => Navigator.pop(ctx, true),
            ),
          );
        });
      })));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Yes'));
      await tester.pumpAndSettle();
      expect(await pending, isTrue, reason: 'primary resolves the dialog true');
    });

    testWidgets('cancel dismisses without confirming', (tester) async {
      late Future<bool?> pending;
      await tester.pumpWidget(_host(Builder(builder: (context) {
        return PcButton('open', onPressed: () {
          pending = showDialog<bool>(
            context: context,
            builder: (ctx) => PcDialog(
              title: 'T',
              primaryLabel: 'Yes',
              cancelLabel: 'No',
              onPrimary: () => Navigator.pop(ctx, true),
            ),
          );
        });
      })));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('No'));
      await tester.pumpAndSettle();
      expect(find.text('T'), findsNothing, reason: 'cancel closes the dialog');
      expect(await pending, isNot(isTrue),
          reason: 'cancel never resolves confirmed');
    });

    testWidgets('destructive dialog renders a destructive primary',
        (tester) async {
      await openAndReturn(
          tester,
          (ctx) => PcDialog(
                title: 'Resign?',
                cancelLabel: 'Cancel',
                primaryLabel: 'Resign',
                destructive: true,
                onPrimary: () => Navigator.pop(ctx, true),
              ));
      final primary =
          tester.widget<PcButton>(find.widgetWithText(PcButton, 'Resign'));
      expect(primary.variant, PcButtonVariant.destructive);
    });

    testWidgets('single-action dialog has no cancel', (tester) async {
      await openAndReturn(
          tester,
          (ctx) => PcDialog(
                title: 'Notice',
                primaryLabel: 'OK',
                onPrimary: () => Navigator.pop(ctx),
              ));
      expect(find.text('OK'), findsOneWidget);
      expect(find.text('Cancel'), findsNothing);
    });
  });

  group('PcChip', () {
    testWidgets('renders its label and fires onTap', (tester) async {
      var taps = 0;
      await tester
          .pumpWidget(_host(PcChip('Wi-Fi', onTap: () => taps++)));
      expect(find.text('Wi-Fi'), findsOneWidget);
      await tester.tap(find.text('Wi-Fi'));
      expect(taps, 1);
    });

    testWidgets('is disabled when onTap is null', (tester) async {
      await tester.pumpWidget(_host(const PcChip('Locked')));
      final button =
          tester.widget<OutlinedButton>(find.byType(OutlinedButton));
      expect(button.onPressed, isNull);
    });

    testWidgets('selected fills gold, unselected outlines muted',
        (tester) async {
      await tester.pumpWidget(_host(PcChip('S', selected: true, onTap: () {})));
      final sel = tester.widget<OutlinedButton>(find.byType(OutlinedButton));
      final selBg = sel.style?.backgroundColor?.resolve({});
      expect(selBg, isNotNull,
          reason: 'a selected chip has a gold fill');

      await tester.pumpWidget(_host(PcChip('U', onTap: () {})));
      final un = tester.widget<OutlinedButton>(find.byType(OutlinedButton));
      final unBg = un.style?.backgroundColor?.resolve({});
      expect(unBg, isNull, reason: 'an unselected chip has no fill');
    });
  });

  group('SeatTile', () {
    testWidgets('shows name, tags and the cash figures', (tester) async {
      await tester.pumpWidget(_host(const SeatTile(
        seat: 0,
        name: 'Alice',
        tags: '(you)',
        active: false,
        bankrupt: false,
        cash: r'$1200',
        vpLabel: 'VP 8/20',
      )));
      expect(find.text('Alice (you)'), findsOneWidget);
      expect(find.text(r'$1200'), findsOneWidget);
      expect(find.text('VP 8/20'), findsOneWidget);
    });

    testWidgets('an active seat shows the turn marker', (tester) async {
      await tester.pumpWidget(_host(const SeatTile(
          seat: 0,
          name: 'A',
          tags: '',
          active: true,
          bankrupt: false,
          cash: r'$1')));
      expect(find.byIcon(Icons.play_arrow), findsOneWidget,
          reason: 'the acting seat is marked');
    });

    testWidgets('rank 1 shows the crown, other ranks a number', (tester) async {
      await tester.pumpWidget(_host(const SeatTile(
          seat: 0,
          name: 'A',
          tags: '',
          active: false,
          bankrupt: false,
          rank: 1,
          cash: r'$1')));
      expect(find.byIcon(Icons.workspace_premium), findsOneWidget);

      await tester.pumpWidget(_host(const SeatTile(
          seat: 0,
          name: 'A',
          tags: '',
          active: false,
          bankrupt: false,
          rank: 3,
          cash: r'$1')));
      expect(find.text('3'), findsOneWidget, reason: 'non-leaders show a rank');
    });

    testWidgets('a bankrupt seat is struck through', (tester) async {
      await tester.pumpWidget(_host(const SeatTile(
          seat: 0,
          name: 'Gone',
          tags: '',
          active: false,
          bankrupt: true,
          cash: r'$0')));
      final text = tester.widget<Text>(find.text('Gone'));
      expect(text.style?.decoration, TextDecoration.lineThrough);
    });

    testWidgets('a lobby seat (no cash) shows no figures column',
        (tester) async {
      await tester.pumpWidget(_host(const SeatTile(
          seat: 0, name: 'Open', tags: '', active: false, bankrupt: false)));
      expect(find.text('Open'), findsOneWidget);
      expect(find.textContaining(r'$'), findsNothing,
          reason: 'no cash before the game starts');
    });
  });

  testWidgets('interactive components participate in keyboard focus',
      (tester) async {
    await tester.pumpWidget(_host(FocusTraversalGroup(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        PcButton('A', onPressed: () {}, wide: false),
        PcButton('B', onPressed: () {}, wide: false),
      ]),
    )));
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pumpAndSettle();
    expect(tester.binding.focusManager.primaryFocus?.hasPrimaryFocus, isTrue,
        reason: 'Tab moves focus onto a button (keyboard/controller nav)');
  });

  testWidgets('Design Showcase renders without overflow at 1024x600',
      (tester) async {
    tester.view.physicalSize = const Size(1024, 600);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(const MaterialApp(home: ShowcaseScreen()));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('PcButton'), findsOneWidget);
  });
}
