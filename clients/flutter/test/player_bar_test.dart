/// The top player bar (DDR-0021) renders every seat as a compact SeatTile with
/// its cash and VP, reusing the ratified component (CAR-0001) in `compact` mode.
/// Guards the seat move out of the side panel into the bar.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/design/components/seat_tile.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/game/player_bar.dart';

Map<String, dynamic> _content() => {
      'mods': [
        {'id': 'base'}
      ],
      'content': {
        'board': [
          {'id': 't0', 'name': 'Go', 'kind': {'type': 'go'}},
        ],
        'rules': {'win_victory_points': 20},
        'market_events': <dynamic>[],
      },
    };

Map<String, dynamic> _view() => {
      'phase': {'type': 'active'},
      'players': [
        for (var p = 0; p < 3; p++)
          {
            'id': 'guest:$p',
            'name': 'Player $p',
            'cash': 1000 + p * 100,
            'position': 0,
            'in_jail': false,
            'bankrupt': false,
            'hand': [2, 3],
            'victory_points': p * 2,
            'hands_cycled': 0,
          },
      ],
      'current': 0, // seat 0 is active
      'turn': {'type': 'await_move'},
      'tiles': [
        {'owner': null, 'houses': 0, 'mortgaged': false},
      ],
      'pending_trades': <dynamic>[],
    };

GameSession _room() => GameSession()
  ..content = GameContent.fromJson(_content())
  ..view = ClientView.fromJson(_view())
  ..seat = 0
  ..seats = [
    for (var p = 0; p < 3; p++)
      SeatInfo.fromJson({
        'seat': p,
        'player_id': 'guest:$p',
        'name': 'Player $p',
        'connected': true,
        'is_bot': false,
      }),
  ];

void main() {
  testWidgets('the player bar renders every seat as a compact cell', (tester) async {
    final s = _room();
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(body: PlayerBar(s: s)),
    ));
    await tester.pump();

    // One SeatTile per seat, all in the compact bar layout.
    final tiles = tester.widgetList<SeatTile>(find.byType(SeatTile)).toList();
    expect(tiles.length, 3);
    expect(tiles.every((t) => t.compact), isTrue);
    // Seat 0 is the acting seat.
    expect(tiles[0].active, isTrue);
    expect(tiles[1].active, isFalse);

    // Identity + live cash are on screen (name may carry a status tag).
    expect(find.textContaining('Player 2'), findsWidgets);
    expect(find.text(r'$1000'), findsOneWidget); // seat 0 cash
    expect(find.text(r'$1200'), findsOneWidget); // seat 2 cash
  });
}
