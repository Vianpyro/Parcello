/// Layout headroom: the game screen must not overflow at the resolutions we
/// ship to. A `RenderFlex overflowed` is a layout *error* in a pumped frame,
/// so simply rendering at each size is the assertion.
///
/// The room here is deliberately loaded - six open trade offers, six seats, a
/// running clock - because the side panel grows with the room, and that is what
/// actually broke: six offers overflowed the panel by 527px on a Steam Deck,
/// which is not a small-screen problem at all. One offer stacks three
/// give-tiles (`TradeOfferCard`'s tallest shape, CAR-0002) so the panel is
/// exercised at its real worst case, not just its uniform one.
///
/// Floor: 1024x600 (measured). Below that the board's centre cannot hold the
/// HUD - 800x600 overflows by ~72px vertically - and a 32-tile board is barely
/// playable anyway.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/ui/game/game_screen.dart';
import 'package:parcello_client/ui/menu/menu_screen.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';

/// The 32-tile ring `mods/base` ships (9x9), so the board's centre - where the
/// HUD lives - is the size the real game gives it.
Map<String, dynamic> _content() => {
  'mods': [
    {'id': 'base'},
  ],
  'content': {
    'board': [
      for (var i = 0; i < 32; i++)
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
    'rules': {
      'expropriation': 120,
      'rent_boost': 50,
      'win_victory_points': 20,
      'subsidiary_pool_factor': 6,
      'conglomerate_pool_factor': 3,
    },
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
    for (var i = 0; i < 32; i++)
      {
        'owner': (i == 5 || i == 6 || i == 7) ? 1 : null,
        'houses': 0,
        'mortgaged': false,
      },
  ],
  // Six simultaneous cards (TradeOfferCard, CAR-0002), deliberately not
  // uniform: #0 stacks three give-tiles (the tallest, worst-case shape -
  // the deferred layout-test gap this fixture now closes), #1 exercises
  // the one-line collapse (cash + exactly one tile), the rest stay
  // cash-only as before.
  'pending_trades': [
    {
      'id': 0,
      'from': 1,
      'to': 0,
      'give_cash': 100,
      // Board indices, not tile ids - `TradeOffer.giveTiles` is `List<int>`
      // (protocol.dart), unlike `TradeDialog`'s own outgoing `propose_trade`
      // which sends string tile ids.
      'give_tiles': [5, 6, 7],
      'receive_cash': 50,
      'receive_tiles': <dynamic>[],
    },
    {
      'id': 1,
      'from': 1,
      'to': 0,
      'give_cash': 100,
      'give_tiles': [5],
      'receive_cash': 50,
      'receive_tiles': <dynamic>[],
    },
    for (var i = 2; i < 6; i++)
      {
        'id': i,
        'from': 1,
        'to': 0,
        'give_cash': 100,
        'give_tiles': <dynamic>[],
        'receive_cash': 50,
        'receive_tiles': <dynamic>[],
      },
  ],
  'subsidiaries_available': 14,
  'conglomerates_available': 7,
};

GameSession _loadedRoom() => GameSession()
  ..content = GameContent.fromJson(_content())
  ..view = ClientView.fromJson(_view())
  ..seat = 0
  ..gameEndsAt = DateTime.now().add(const Duration(minutes: 30))
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

/// The sizes we commit to: the Steam Deck, the default desktop window, and the
/// smallest laptop the layout still holds together on.
const _sizes = <String, Size>{
  'Steam Deck 1280x800': Size(1280, 800),
  'default window 1280x720': Size(1280, 720),
  'floor 1024x600': Size(1024, 600),
};

void main() {
  for (final entry in _sizes.entries) {
    testWidgets('game screen fits: ${entry.key}', (tester) async {
      tester.view.physicalSize = entry.value;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);

      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: GameScreen(s: _loadedRoom()),
        ),
      );
      // Settle the entry animations; an overflow throws during layout.
      await tester.pump(const Duration(milliseconds: 400));
    });
  }

  testWidgets('menu fits at the floor', (tester) async {
    tester.view.physicalSize = const Size(1024, 600);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: MenuScreen(s: GameSession()),
      ),
    );
    await tester.pumpAndSettle();
  });
}
