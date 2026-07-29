/// The board's PAINT ORDER (DDR-0022, step C3).
///
/// Steps A/B proved the projection maths and C0-C2 proved the composition. What
/// neither covers is the order the cards are drawn in - and once the board is
/// projected, that order is visible: the projected cell is a 2:1 diamond, a card
/// is `1.5` cells wide ([BoardGeometry] `_cardFootprint`), so every pair of ring
/// neighbours overlaps by about a fifth of a card. The flat board never had to
/// care (its cells were disjoint, the paint order meant nothing), so the
/// renderer paints in ring-index order - and the ring index is NOT monotone in
/// depth: it walks from the nearest tile to the farthest and back, so on one
/// half of the board the farther card is painted over the nearer one.
///
/// These tests are geometric, not golden: "the nearer of two overlapping cards
/// is painted last" is checkable, and the tile index is recovered from what the
/// board actually placed on screen rather than from a second copy of its
/// formulas.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/board.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/stage.dart';

/// The 32-tile ring `mods/base` ships (9x9).
const _tiles = 32;

/// Boxes to render into. The paint order must be the same in all of them - it
/// is a property of the projection, not of the viewport (which is also what a
/// later fixed-logical-size board and a zoom/pan camera need).
const _boxes = <Size>[
  Size(820, 560),
  Size(600, 600),
  Size(1000, 420),
];

Map<String, dynamic> _content() => {
      'mods': [
        {'id': 'base'}
      ],
      'content': {
        'board': [
          for (var i = 0; i < _tiles; i++)
            {
              'id': 't$i',
              'name': 'Tile $i',
              'kind': i == 0
                  ? {'type': 'go'}
                  : i == 8
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
        'rules': {'win_victory_points': 20},
        'market_events': <dynamic>[],
      },
    };

Map<String, dynamic> _view() => {
      'phase': {'type': 'active'},
      'players': [
        for (var p = 0; p < 2; p++)
          {
            'id': 'guest:$p',
            'name': 'Player $p',
            'cash': 1200,
            'position': p * 3,
            'in_jail': false,
            'bankrupt': false,
            'hand': [2, 3, 4],
            'victory_points': p,
            'hands_cycled': 0,
          },
      ],
      'current': 0,
      'turn': {'type': 'await_move', 'bids': <dynamic>[]},
      'tiles': [
        for (var i = 0; i < _tiles; i++)
          {'owner': null, 'houses': 0, 'mortgaged': false},
      ],
      'pending_trades': <dynamic>[],
    };

/// The board alone in a box of a known size: this is about what the board draws
/// inside its own surface, so nothing of the surrounding screen is involved.
Widget _harness(Size box, {void Function(int tile)? onTileTap}) => MaterialApp(
      home: Scaffold(
        body: Center(
          child: SizedBox(
            width: box.width,
            height: box.height,
            child: BoardWidget(
              content: GameContent.fromJson(_content()),
              view: ClientView.fromJson(_view()),
              mySeat: 0,
              onTileTap: onTileTap ?? (_) {},
              canAct: (_) => false,
              stage: StageState(),
              center: const SizedBox.shrink(),
            ),
          ),
        ),
      ),
    );

/// The Stack the ring is painted in: the only one in the board's subtree with a
/// child per tile plus the pawn layer.
Stack _ringStack(WidgetTester tester) {
  final found = tester
      .widgetList<Stack>(find.descendant(
          of: find.byType(BoardWidget), matching: find.byType(Stack)))
      .where((s) => s.children.length == _tiles + 1)
      .toList();
  expect(found, hasLength(1),
      reason: 'expected exactly one ring Stack (${_tiles + 1} children)');
  return found.single;
}

/// The geometry the board actually rendered with. The surface is MEASURED
/// (`boardSurfaceKey`, the precedent step C0 set) rather than re-derived.
BoardGeometry _geometry(WidgetTester tester) => BoardGeometry.of(
    tiles: _tiles, box: tester.getSize(find.byKey(boardSurfaceKey)));

String _at(double x, double y) =>
    '${x.toStringAsFixed(3)},${y.toStringAsFixed(3)}';

/// The tile index of every ring child, in the order the Stack paints them
/// (first child = painted first = underneath).
///
/// The index is recovered from the child's own placement, so this needs nothing
/// of the board but the cards it puts on screen - it has to work before any key
/// or paint-order API exists.
List<int> _paintedOrder(WidgetTester tester, BoardGeometry g) {
  final byTopLeft = <String, int>{
    for (var i = 0; i < _tiles; i++)
      _at(g.centreOf(i).dx - g.tileSize / 2, g.centreOf(i).dy - g.tileSize / 2):
          i,
  };
  final order = <int>[];
  for (final child in _ringStack(tester).children) {
    // The pawn layer is a Positioned.fill: no width, and not a card.
    if (child is! Positioned || child.width == null) continue;
    final i = byTopLeft[_at(child.left!, child.top!)];
    expect(i, isNotNull,
        reason: 'a ring child at ${child.left},${child.top} matches no tile');
    order.add(i!);
  }
  expect(order, hasLength(_tiles));
  return order;
}

Rect _card(BoardGeometry g, int i) => Rect.fromCenter(
    center: g.centreOf(i), width: g.tileSize, height: g.tileSize);

/// Two cards overlap by strictly more than a rounding error.
///
/// The margin is not slack, it makes the pair set DETERMINISTIC: a card is 1.5
/// projected units tall and some ring pairs sit exactly 1.5 apart, so they touch
/// without overlapping - and whether floating point puts such a pair in or out
/// then depends on the box the board was fitted into. Cards that merely touch
/// cannot occlude each other, so they are excluded on purpose.
bool _overlap(BoardGeometry g, int a, int b) =>
    _card(g, a).deflate(0.01).overlaps(_card(g, b).deflate(0.01));

/// Every pair of tiles whose cards overlap on screen - the pairs, and only the
/// pairs, whose paint order is observable.
List<List<int>> _overlappingPairs(BoardGeometry g) {
  final pairs = <List<int>>[];
  for (var a = 0; a < g.tiles; a++) {
    for (var b = a + 1; b < g.tiles; b++) {
      if (_overlap(g, a, b)) pairs.add([a, b]);
    }
  }
  return pairs;
}

/// Every overlapping pair drawn in the wrong order under `order`, as readable
/// sentences (an empty list is the invariant).
List<String> _misordered(BoardGeometry g, List<int> order) {
  final slot = {for (var s = 0; s < order.length; s++) order[s]: s};
  final wrong = <String>[];
  for (final pair in _overlappingPairs(g)) {
    final a = pair[0], b = pair[1];
    final ya = g.centreOf(a).dy, yb = g.centreOf(b).dy;
    if (ya == yb) continue; // same depth: neither is in front of the other
    final nearer = ya > yb ? a : b; // greater screen y = closer to the eye
    final later = slot[a]! > slot[b]! ? a : b;
    if (later != nearer) {
      wrong.add('$later (y=${ya > yb ? yb : ya}) is painted over '
          '$nearer (y=${ya > yb ? ya : yb})');
    }
  }
  return wrong;
}

void main() {
  for (final box in _boxes) {
    testWidgets('the nearer of two overlapping cards is painted last: $box',
        (tester) async {
      tester.view.physicalSize = box * 1.5;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(_harness(box));
      await tester.pump(const Duration(milliseconds: 400));

      final g = _geometry(tester);
      final pairs = _overlappingPairs(g);

      // Guard the guard: if the cards did not overlap, the paint order would be
      // unobservable and every assertion below would pass for nothing.
      expect(pairs, isNotEmpty,
          reason: 'no two cards overlap - the paint order proves nothing');

      final wrong = _misordered(g, _paintedOrder(tester, g));
      expect(wrong, isEmpty,
          reason: '${wrong.length} of ${pairs.length} overlapping pairs are '
              'painted far-over-near:\n${wrong.join('\n')}');
    });
  }

  testWidgets('Go is the nearest tile, so it is painted over its neighbours',
      (tester) async {
    // Tile 0 is the ring's bottom vertex - the cell closest to the eye and the
    // one card the player must never see buried. Its two neighbours (1 and 31)
    // are one step BEHIND it, and in ring-index order both are painted after it.
    await tester.pumpWidget(_harness(_boxes.first));
    await tester.pump(const Duration(milliseconds: 400));

    final g = _geometry(tester);
    final slot = {
      for (final e in _paintedOrder(tester, g).asMap().entries) e.value: e.key
    };

    expect(_card(g, 0).overlaps(_card(g, 1)), isTrue);
    expect(_card(g, 0).overlaps(_card(g, 31)), isTrue);
    expect(g.centreOf(0).dy, greaterThan(g.centreOf(1).dy));
    expect(g.centreOf(0).dy, greaterThan(g.centreOf(31).dy));

    expect(slot[0], greaterThan(slot[1]!),
        reason: 'tile 1 is behind Go and is painted over it');
    expect(slot[0], greaterThan(slot[31]!),
        reason: 'tile 31 is behind Go and is painted over it');
  });

  testWidgets('the board paints exactly paintOrder, pawns last',
      (tester) async {
    await tester.pumpWidget(_harness(_boxes.first));
    await tester.pump(const Duration(milliseconds: 400));

    final g = _geometry(tester);
    final children = _ringStack(tester).children;

    // 1. What is drawn is the order, not a copy of it that could drift.
    expect(_paintedOrder(tester, g), g.paintOrder);

    // 2. Each card carries its tile's key, so its element and the animation
    //    state inside it follow the TILE and not the slot in the list.
    for (var s = 0; s < g.paintOrder.length; s++) {
      expect(children[s].key, ValueKey(g.paintOrder[s]),
          reason: 'slot $s does not carry its tile key');
    }

    // 3. The pawn layer is still the last child: pawns above every card.
    expect(children.last, isA<Positioned>());
    expect((children.last as Positioned).width, isNull,
        reason: 'the last child is a card, not the pawn layer');
    expect(find.descendant(of: find.byWidget(children.last),
            matching: find.byType(DecoratedBox)),
        findsWidgets,
        reason: 'the last child draws no pawn');
  });

  testWidgets('a tap where two cards overlap lands on the one in front',
      (tester) async {
    // Nothing about hit-testing is implemented here - a Stack tests its children
    // in reverse paint order, so painting back-to-front makes the card the
    // player SEES on top the card that answers. It is asserted because it is a
    // behaviour change: before, on half the board, the buried card won the tap.
    final taps = <int>[];
    await tester.pumpWidget(_harness(_boxes.first, onTileTap: taps.add));
    await tester.pump(const Duration(milliseconds: 400));

    final g = _geometry(tester);
    final front = _card(g, 0); // Go: the bottom vertex, nearest the eye
    final behind = _card(g, 1); // one step back up the ring
    expect(g.centreOf(0).dy, greaterThan(g.centreOf(1).dy));

    final shared = front.intersect(behind).center;
    expect(front.contains(shared), isTrue);
    expect(behind.contains(shared), isTrue);

    await tester.tapAt(tester.getTopLeft(find.byKey(boardSurfaceKey)) + shared);
    await tester.pump();

    expect(taps, [0], reason: 'the tap went to the card behind');
  });

  group('paintOrder is pure geometry', () {
    // Layout-free and scale-free: the order is a property of the projection, so
    // it is asserted without a widget at all. The ring sizes are the ones
    // `board_geometry_test` uses, and the boxes deliberately include the
    // degenerate shapes.
    const rings = [8, 32, 40, 60];
    const boxes = <Size>[
      Size(400, 400),
      Size(900, 300),
      Size(300, 900),
      Size(820, 560),
      Size(101, 997),
    ];

    test('it is a permutation of the ring', () {
      for (final tiles in rings) {
        final order = BoardGeometry.of(tiles: tiles, box: boxes.first).paintOrder;
        expect(order, hasLength(tiles));
        expect(order.toSet(), hasLength(tiles), reason: 'ring $tiles repeats');
        expect(order.toSet(), {for (var i = 0; i < tiles; i++) i},
            reason: 'ring $tiles is missing a tile');
      }
    });

    test('it is strictly increasing in depth', () {
      for (final tiles in rings) {
        for (final box in boxes) {
          final g = BoardGeometry.of(tiles: tiles, box: box);
          for (var s = 1; s < g.paintOrder.length; s++) {
            final before = g.centreOf(g.paintOrder[s - 1]);
            final after = g.centreOf(g.paintOrder[s]);
            expect(after.dy, greaterThanOrEqualTo(before.dy),
                reason: 'ring $tiles in $box goes forward then back at slot $s');
            // Total, not just monotone: equal depth is broken by x, so the order
            // never depends on an unstable sort.
            expect(after.dy > before.dy || after.dx > before.dx, isTrue,
                reason: 'ring $tiles in $box ties at slot $s');
          }
        }
      }
    });

    test('the nearer of two overlapping cards always comes later', () {
      for (final tiles in rings) {
        for (final box in boxes) {
          final g = BoardGeometry.of(tiles: tiles, box: box);
          expect(_overlappingPairs(g), isNotEmpty,
              reason: 'ring $tiles in $box has no overlap to order');
          final wrong = _misordered(g, g.paintOrder);
          expect(wrong, isEmpty,
              reason: 'ring $tiles in $box:\n${wrong.join('\n')}');
        }
      }
    });

    test('it does not depend on the box', () {
      // The same invariant `tileSize` has, for the same reason: the board will
      // later be a scene with a fixed logical size drawn through a zoom/pan
      // camera, and depth is a rank - no viewport may reorder it. Opening a
      // panel must not reshuffle the board either.
      for (final tiles in rings) {
        final reference = BoardGeometry.of(tiles: tiles, box: boxes.first);
        for (final box in boxes.skip(1)) {
          expect(BoardGeometry.of(tiles: tiles, box: box).paintOrder,
              reference.paintOrder,
              reason: 'ring $tiles reordered in $box');
        }
        // And it is the free function's answer, not a second derivation.
        expect(reference.paintOrder, ringPaintOrder(tiles));
      }
    });

    test('Go is painted last on the shipping ring', () {
      // Tile 0 is the bottom vertex: the nearest card, so the last one drawn.
      expect(BoardGeometry.of(tiles: _tiles, box: boxes.first).paintOrder.last,
          0);
    });
  });
}
