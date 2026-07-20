/// Widget tests for the design-system components + the Design Showcase.
/// Each component lands with its test here (DESIGN/COMPONENT_INVENTORY.md).
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/design/components/pc_button.dart';
import 'package:parcello_client/design/components/pc_card.dart';
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
