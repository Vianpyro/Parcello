/// The render path for the motion language: the board's attention devices, and
/// the overlay that carries money from a tile to a seat marker.
///
/// `director_test.dart` proves the *plan* is right; this proves the plan can
/// actually be drawn. A compiler that emits a beat nothing can render is worth
/// nothing, and only a pumped frame catches a layout assert.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/board.dart';
import 'package:parcello_client/motion.dart';
import 'package:parcello_client/overlay.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/stage.dart';
import 'package:parcello_client/tokens.dart';

/// An 8-tile ring (the smallest `isSquareRing`), enough to exercise the
/// geometry without a full mod.
Map<String, dynamic> content() => {
      'mods': [
        {'id': 'base'}
      ],
      'content': {
        'board': [
          for (var i = 0; i < 8; i++)
            {
              'id': 't$i',
              'name': 'Tile $i',
              'kind': i == 0
                  ? {'type': 'go'}
                  : i == 2
                      ? {'type': 'jail'}
                      : {
                          'type': 'property',
                          'group': 'navy',
                          'price': 100 + i,
                          'house_cost': 50,
                          'rent_model': 'houses',
                        },
            },
        ],
        'rules': {'expropriation': 120, 'rent_boost': 50, 'win_victory_points': 20},
        'market_events': <dynamic>[],
      },
    };

Map<String, dynamic> view({
  String turn = 'await_move',
  Map<String, dynamic>? market,
}) =>
    {
      'phase': {'type': 'active'},
      if (market != null)
        'forecast': {'queue': <dynamic>[], 'active': market},
      'players': [
        {
          'id': 'guest:a',
          'name': 'Alice',
          'cash': 1200,
          'position': 1,
          'in_jail': false,
          'bankrupt': false,
          'hand': [2, 3],
          'victory_points': 4,
        },
        {
          'id': 'guest:b',
          'name': 'Bob',
          'cash': 900,
          'position': 5,
          'in_jail': false,
          'bankrupt': false,
          'hand': [4],
          'victory_points': 1,
        },
      ],
      'current': 0,
      'turn': {'type': turn, if (turn == 'blind_auction') 'tile': 3, 'bids': [null, null]},
      'tiles': [
        for (var i = 0; i < 8; i++)
          {'owner': i == 5 ? 1 : null, 'houses': 0, 'mortgaged': false},
      ],
      'pending_trades': <dynamic>[],
    };

/// The board plus the overlay, in the same z-order the game uses: chits are
/// drawn above both the board and the HUD, because that is the only place from
/// which they can cross between them.
Widget harness(StageState stage, {Map<String, dynamic>? market}) {
  final c = GameContent.fromJson(content());
  final v = ClientView.fromJson(view(market: market));
  return MaterialApp(
    home: Scaffold(
      backgroundColor: Pc.bg,
      body: Stack(children: [
        Row(children: [
          Expanded(
            child: BoardWidget(
              content: c,
              view: v,
              mySeat: 0,
              onTileTap: (_) {},
              canAct: (_) => false,
              stage: stage,
              center: const SizedBox.shrink(),
            ),
          ),
          // Stands in for the side panel: the seat markers money lands on.
          SizedBox(
            width: 200,
            child: Column(children: [
              for (var i = 0; i < 2; i++)
                SizedBox(
                    key: stage.anchors.seatKey(i), width: 18, height: 18),
            ]),
          ),
        ]),
        StageOverlay(stage: stage),
      ]),
    ),
  );
}

void main() {
  testWidgets('the board renders and installs a tile anchor', (tester) async {
    final stage = StageState();
    await tester.pumpWidget(harness(stage));
    expect(find.text('Tile 1'), findsOneWidget);
    // Without this, money has nowhere to fly from.
    expect(stage.anchors.resolve(const TileAnchor(3)), isNotNull);
    expect(stage.anchors.resolve(const SeatAnchor(1)), isNotNull);
  });

  testWidgets('a chit states itself before it travels, then travels',
      (tester) async {
    final stage = StageState();
    await tester.pumpWidget(harness(stage));

    stage.addChit(
      from: const TileAnchor(5),
      to: const SeatAnchor(0),
      text: r'+$120',
      kind: ChitKind.gain,
    );
    await tester.pump();
    expect(find.text(r'+$120'), findsOneWidget);
    final source = tester.getCenter(find.text(r'+$120'));

    // Phase 1 - it holds at its source for the whole hold, so the player can
    // read how much and from where before anything moves. A chit that sets off
    // immediately is a number you have to chase.
    await tester.pump(Motion.chitHold - const Duration(milliseconds: 40));
    expect(tester.getCenter(find.text(r'+$120')).dx, closeTo(source.dx, 0.5),
        reason: 'nothing moves during the hold');

    // Phase 2 - now it goes somewhere.
    await tester.pump(const Duration(milliseconds: 250));
    expect(tester.getCenter(find.text(r'+$120')).dx, isNot(closeTo(source.dx, 1)));

    // And it is gone once its own animation runs out - the stage must not leak
    // chits, or a long game accumulates them forever.
    await tester.pumpAndSettle();
    expect(stage.chits, isEmpty);
    expect(find.text(r'+$120'), findsNothing);
  });

  test('the chit beat is paid for exactly as long as the chit is rendered', () {
    // A mismatch would let the plan finish - and the ADR-0028 ack fire,
    // releasing the server's timers - while money is still in the air.
    expect(Motion.chitHold + Motion.chitTravel, Motion.chit);
  });

  testWidgets('recede dims the board but never the subject', (tester) async {
    final stage = StageState();
    await tester.pumpWidget(harness(stage));

    stage
      ..focusTile = 3
      ..recede = true
      ..bump();
    await tester.pumpAndSettle();

    double opacityOf(int tile) => tester
        .widget<AnimatedOpacity>(find.ancestor(
          of: find.text('Tile $tile'),
          matching: find.byType(AnimatedOpacity),
        ))
        .opacity;

    expect(opacityOf(3), 1.0, reason: 'the tile being decided on stays lit');
    expect(opacityOf(1), lessThan(1.0), reason: 'everything else steps back');
  });

  testWidgets('the P1 arrest states what happened and to whom', (tester) async {
    final stage = StageState();
    await tester.pumpWidget(harness(stage));

    stage
      ..arrest = const Arrest(
          title: 'Bob is bankrupt', detail: 'Alice takes the estate.', seat: 1)
      ..recede = true
      ..bump();
    await tester.pumpAndSettle();

    expect(find.text('BOB IS BANKRUPT'), findsOneWidget);
    expect(find.text('Alice takes the estate.'), findsOneWidget);
  });

  testWidgets('a market event moves the price printed on the tiles',
      (tester) async {
    // While an acquisition multiplier is active, the list price on the tile is
    // simply not the price. The forecast strip promised this three turns ago;
    // the board is where the promise gets kept.
    final stage = StageState();
    await tester.pumpWidget(harness(stage, market: {
      'event_id': 'market_bubble',
      'effect': 'acquisition_multiplier',
      'magnitude_pct': -30,
      'ends_at_turn': 40,
    }));

    // Tile 4 lists at $104; a -30% bubble makes it $72 to take.
    expect(find.textContaining(r'$72 (was $104)'), findsOneWidget);
    expect(find.textContaining(r'$104  '), findsNothing);
  });

  testWidgets('a rent-only market event leaves prices alone', (tester) async {
    // Market Crash scales rent, not acquisition. Showing a moved price here
    // would be a lie, and the grammar only works while it never lies.
    final stage = StageState();
    await tester.pumpWidget(harness(stage, market: {
      'event_id': 'market_crash',
      'effect': 'rent_multiplier',
      'magnitude_pct': -50,
      'ends_at_turn': 40,
    }));

    expect(find.textContaining('was'), findsNothing);
  });

  testWidgets('a refused command shakes the tile that refused it',
      (tester) async {
    // An error in a log the player is not reading is an error they have to hunt
    // for. It belongs on the thing that said no.
    final stage = StageState();
    await tester.pumpWidget(harness(stage));

    final subject = tester.getCenter(find.text('Tile 4'));
    final bystander = tester.getCenter(find.text('Tile 1'));

    stage.refuse(4, 'not_your_turn');
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 60));

    expect(tester.getCenter(find.text('Tile 4')).dx, isNot(subject.dx));
    expect(tester.getCenter(find.text('Tile 1')).dx, bystander.dx,
        reason: 'only the tile that refused reacts');

    // And it settles back exactly where it was: a refusal, not a tantrum.
    await tester.pumpAndSettle();
    expect(tester.getCenter(find.text('Tile 4')).dx, closeTo(subject.dx, 0.01));
  });

  testWidgets('reduced motion keeps the information and drops the journey',
      (tester) async {
    final stage = StageState()..profile = MotionProfile.reduced;
    await tester.pumpWidget(harness(stage));

    stage.addChit(
      from: const TileAnchor(5),
      to: const SeatAnchor(0),
      text: r'-$80',
      kind: ChitKind.loss,
    );
    await tester.pump();

    // The number, the sign and the colour are all still there - it simply does
    // not travel. No information exists only in motion (ADR-0030).
    expect(find.text(r'-$80'), findsOneWidget);
    expect(stage.chits.single.travels, isFalse);
    await tester.pumpAndSettle();
  });
}
