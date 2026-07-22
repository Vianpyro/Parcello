/// A movement card must outline its destination on keyboard / controller
/// FOCUS, not only on mouse hover - otherwise a Steam Deck or keyboard player
/// (no hover) chooses a card blind. `ActionsPanel` wraps each card button so
/// focusing it previews the landing tile exactly as a hover does.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/game/actions_panel.dart';

Map<String, dynamic> _content() => {
      'mods': [
        {'id': 'base'}
      ],
      'content': {
        'board': [
          for (var i = 0; i < 32; i++)
            {
              'id': 't$i',
              'name': 'Tile $i',
              'kind': i == 0
                  ? {'type': 'go'}
                  : {
                      'type': 'property',
                      'group': 'navy',
                      'price': 100,
                      'house_cost': 50,
                      'rent_model': 'houses',
                    },
            },
        ],
        'rules': {'win_victory_points': 20},
        'market_events': <dynamic>[],
      },
    };

// Seat 0's turn to move, holding a fresh hand, standing on Go.
Map<String, dynamic> _view() => {
      'phase': {'type': 'active'},
      'players': [
        for (var p = 0; p < 2; p++)
          {
            'id': 'guest:$p',
            'name': 'Player $p',
            'cash': 1500,
            'position': 0,
            'in_jail': false,
            'bankrupt': false,
            'hand': [2, 3],
            'victory_points': 0,
            'hands_cycled': 0,
          },
      ],
      'current': 0,
      'turn': {'type': 'await_move'},
      'tiles': [
        for (var i = 0; i < 32; i++)
          {'owner': null, 'houses': 0, 'mortgaged': false},
      ],
      'pending_trades': <dynamic>[],
    };

GameSession _room() => GameSession()
  ..content = GameContent.fromJson(_content())
  ..view = ClientView.fromJson(_view())
  ..seat = 0
  ..seats = [
    for (var p = 0; p < 2; p++)
      SeatInfo.fromJson({
        'seat': p,
        'player_id': 'guest:$p',
        'name': 'Player $p',
        'connected': true,
        'is_bot': false,
      }),
  ];

void main() {
  testWidgets('focusing a movement card previews its destination tile',
      (tester) async {
    final s = _room();
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(body: ActionsPanel(s: s)),
    ));
    await tester.pump();

    // The hand renders as buttons; nothing is previewed before any focus.
    expect(find.text('2'), findsOneWidget);
    expect(s.hoverTile, isNull);

    // Tab moves keyboard / Steam Deck focus onto the first card ('2'); its
    // destination (Go + 2 on a 32-tile ring) must light up like a hover.
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    expect(s.hoverTile, 2);
  });
}
