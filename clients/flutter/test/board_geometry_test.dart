/// The board's COMPOSITION, asserted geometrically (DDR-0022, step C0).
///
/// Steps A and B moved every tile-to-pixel mapping into `IsoProjection` and
/// proved the maths: injective, exactly invertible, correctly fitted. All of
/// that is still true - and the board still came out wrong, because the maths
/// was never the problem. The flat board's *composition* numbers survived the
/// projection and quietly stopped meaning what they used to: the centre plaza
/// kept the `(d-2)/d` rectangle that used to be the ring's hole, and under the
/// projection that rectangle lies ON the ring instead of inside it. At the
/// 1024x600 floor it buried 26 of 32 tiles.
///
/// So this file asserts the layer step B had no test for. It is purely
/// geometric - no golden, no screenshot, nothing that needs an eye. It pumps
/// the real screen for one reason only: to MEASURE the box the board is
/// actually given at each shipping size. A composition test that invents its
/// own box proves nothing about the composition.
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/board.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/game/game_screen.dart';

/// The 32-tile ring `mods/base` ships (9x9).
const _tiles = 32;

/// The sizes we commit to - the same three `layout_test` holds the screen to.
const _sizes = <String, Size>{
  'Steam Deck 1280x800': Size(1280, 800),
  'default window 1280x720': Size(1280, 720),
  'floor 1024x600': Size(1024, 600),
};

/// How much of the width it is handed the board must actually use.
///
/// The board is the protagonist (`docs/motion-language.md` 2), and "the board
/// got smaller" is otherwise a regression nobody notices until a screenshot.
/// The flat-board `AspectRatio(1)` throws the region's surplus width away and
/// then lets the plaza eat what is left; both show up here as one number.
const _minDiamondWidthFraction = 0.85;

/// Sub-pixel slack. The fit's padding is exact, so a vertex card lands exactly
/// on the edge of the surface at the constrained dimension - that is the
/// padding working, not a clip.
const _epsilon = 0.01;

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
        for (var p = 0; p < 6; p++)
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

/// A full table: six seats, because the player bar's height is what the board
/// region gets whatever is left of.
GameSession _room() => GameSession()
  ..content = GameContent.fromJson(_content())
  ..view = ClientView.fromJson(_view())
  ..seat = 0
  ..seats = [
    for (var p = 0; p < 6; p++)
      SeatInfo.fromJson({
        'seat': p,
        'player_id': 'guest:$p',
        'name': 'Player $p',
        'connected': true,
        'is_bot': false,
      }),
  ];

/// True when `p` is strictly inside the convex polygon `poly` (a point exactly
/// on an edge counts as outside - a tile whose centre sits on the plaza's rim
/// is not covered by it).
bool _inside(List<Offset> poly, Offset p) {
  var sign = 0;
  for (var i = 0; i < poly.length; i++) {
    final a = poly[i];
    final b = poly[(i + 1) % poly.length];
    final cross = (b.dx - a.dx) * (p.dy - a.dy) - (b.dy - a.dy) * (p.dx - a.dx);
    if (cross == 0) return false;
    final s = cross > 0 ? 1 : -1;
    if (sign == 0) {
      sign = s;
    } else if (s != sign) {
      return false;
    }
  }
  return true;
}

/// Every corner of `r` strictly inside `poly`.
bool _rectInside(List<Offset> poly, Rect r) =>
    _inside(poly, r.topLeft) &&
    _inside(poly, r.topRight) &&
    _inside(poly, r.bottomRight) &&
    _inside(poly, r.bottomLeft);

void main() {
  for (final entry in _sizes.entries) {
    testWidgets('board composition: ${entry.key}', (tester) async {
      tester.view.physicalSize = entry.value;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);

      await tester.pumpWidget(MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: GameScreen(s: _room()),
      ));
      await tester.pump(const Duration(milliseconds: 400));

      // Measured, not assumed: the box the board is fitted into, and the box
      // the layout offered it. They are the same thing only if the board stops
      // giving width back.
      final surface = tester.getSize(find.byKey(boardSurfaceKey));
      final offered =
          tester.renderObject<RenderBox>(find.byType(BoardWidget)).constraints;
      final region = Size(offered.maxWidth, offered.maxHeight);
      final g = BoardGeometry.of(tiles: _tiles, box: surface);

      // 1. The plaza is the ring's HOLE. It may never contain a tile: that is
      //    the property the flat rectangle had for free and lost in
      //    projection, and it is the whole reason 26 tiles went missing.
      for (var i = 0; i < _tiles; i++) {
        expect(_inside(g.plaza, g.centreOf(i)), isFalse,
            reason: 'tile $i sits inside the plaza (${g.plaza})');
      }

      // 2. Whatever the composition reserves in the board's middle must fit in
      //    that hole. It cannot - the largest aligned rectangle inside the
      //    projected hole is 3/8 x 3/8 of the diamond's box - so the only
      //    passing answer is to reserve nothing there. DDR-0022: the
      //    contextual panel never goes back into the board's centre.
      final slot = g.centreSlot;
      expect(slot == null || _rectInside(g.plaza, slot), isTrue,
          reason: 'the centre slot $slot escapes the plaza and covers tiles');

      // 3. Every tile is inside the board's own surface - the fit's padding is
      //    half a tile precisely so the four vertex cards (Go included) are
      //    not clipped. They may touch the edge: the padding is exact, so at
      //    the constrained dimension they always do.
      for (var i = 0; i < _tiles; i++) {
        final card = Rect.fromCenter(
            center: g.centreOf(i), width: g.tileSize, height: g.tileSize);
        expect(
            card.left >= -_epsilon &&
                card.top >= -_epsilon &&
                card.right <= surface.width + _epsilon &&
                card.bottom <= surface.height + _epsilon,
            isTrue,
            reason: 'tile $i is clipped by the board surface '
                '($card vs ${surface.width}x${surface.height})');
      }

      // 4. And the board is the protagonist: it uses the width it was given
      //    instead of handing it back.
      final used = g.diamond.width / region.width;
      expect(used, greaterThanOrEqualTo(_minDiamondWidthFraction),
          reason: 'the diamond is ${(used * 100).round()}% of the '
              '${region.width.round()}px it was offered '
              '(surface ${surface.width.round()}x${surface.height.round()}, '
              'diamond ${g.diamond.width.round()}x${g.diamond.height.round()})');
    });
  }

  test('the plaza is the ring hole, at every ring size', () {
    // Scale-free and layout-free: whatever box it is fitted into, the plaza is
    // the interior block's rhombus and the ring is outside it. Guards the
    // definition itself, so the sage losange a later step paints from it can
    // never be the thing that eats the board again.
    for (final tiles in [8, 32, 40, 60]) {
      for (final box in const [Size(400, 400), Size(900, 300), Size(300, 900)]) {
        final g = BoardGeometry.of(tiles: tiles, box: box);
        for (var i = 0; i < tiles; i++) {
          expect(_inside(g.plaza, g.centreOf(i)), isFalse,
              reason: 'ring $tiles in $box: tile $i is inside the plaza');
        }
      }
    }
  });

  test('the plaza never exceeds the diamond it sits in', () {
    final g = BoardGeometry.of(tiles: _tiles, box: const Size(600, 400));
    final d = g.diamond;
    for (final p in g.plaza) {
      expect(d.contains(p), isTrue, reason: '$p escapes the diamond $d');
    }
    // And it is a real hole, not a degenerate point.
    final w = g.plaza.map((p) => p.dx).reduce(math.max) -
        g.plaza.map((p) => p.dx).reduce(math.min);
    expect(w, greaterThan(0));
  });

  test('tileSize comes from the projection, not from the box', () {
    // The invariant of DDR-0022 step C1.5, and the only guard on it.
    //
    // `tileSize` used to be `min(box.width, box.height) / d` - a number read
    // off the VIEWPORT. That was only ever right while an `AspectRatio(1)`
    // forced the board's box square, which made "a d-th of the box" and "one
    // grid step" the same number. Once step C1 dropped that square the two
    // drifted: the cards were sized by one dimension of the box while the
    // diamond was scaled by the other, and at the 1024x600 floor the cards
    // came out 29% larger than the step they sit on.
    //
    // So what has to hold is not a number, it is a DEPENDENCY: a card is a
    // fixed multiple of the projected step, therefore `tileSize / fit.scale`
    // is the same in every box. It is deliberately asserted against the first
    // geometry rather than against a literal - the multiple itself is a design
    // decision and does not belong frozen in a test. Any re-coupling to the
    // viewport or to the layout moves this ratio by percents; the tolerance
    // below is for floating-point noise, nothing else.
    const boxes = <Size>[
      // The two shipping surfaces that share a width and differ in height -
      // the exact pair the old coupling handed two different card sizes to.
      Size(832, 587), // Steam Deck
      Size(832, 507), // default 1280x720 window
      Size(576, 387), // the 1024x600 floor
      Size(400, 400), // square: the one shape the old formula got right
      Size(900, 300), // and the shapes it did not
      Size(300, 900),
      Size(1237, 613),
      Size(101, 997),
    ];
    for (final tiles in [8, 32, 40, 60]) {
      final first = BoardGeometry.of(tiles: tiles, box: boxes.first);
      final reference = first.tileSize / first.fit.scale;
      for (final box in boxes.skip(1)) {
        final g = BoardGeometry.of(tiles: tiles, box: box);
        expect(g.tileSize / g.fit.scale, closeTo(reference, 1e-9),
            reason: 'ring $tiles in $box: a card is '
                '${g.tileSize / g.fit.scale} of the projected step, but '
                '$reference in ${boxes.first} - tileSize has picked up a '
                'dependency on the box');
      }
    }
  });
}
