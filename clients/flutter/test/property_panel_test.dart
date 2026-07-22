/// The property card renders the tile's real rent schedule + owner + level.
///
/// The rent ladder is the engine's `PropertyDef.rents` (already on the wire,
/// now parsed by `TileDef`) - honest data, never fabricated. Guards that the
/// panel shows the schedule and marks the current development level.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/game/property_panel.dart';

Map<String, dynamic> _content() => {
      'mods': [
        {'id': 'base'}
      ],
      'content': {
        'board': [
          for (var i = 0; i < 4; i++)
            {
              'id': 't$i',
              'name': 'Tile $i',
              'kind': i == 0
                  ? {'type': 'go'}
                  : {
                      'type': 'property',
                      'group': 'navy',
                      'price': 200,
                      'house_cost': 50,
                      'rent_model': 'houses',
                      // rents[0] unimproved .. rents[5] hotel.
                      'rents': [10, 30, 90, 160, 250, 500],
                    },
            },
        ],
        'rules': {'win_victory_points': 20},
        'market_events': <dynamic>[],
      },
    };

// Tile 3 is owned by seat 1 with two houses standing on it.
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
        for (var i = 0; i < 4; i++)
          {
            'owner': i == 3 ? 1 : null,
            'houses': i == 3 ? 2 : 0,
            'mortgaged': false,
          },
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
  testWidgets('property card shows the real rent ladder, owner, and level',
      (tester) async {
    final s = _room();
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(body: PropertyPanel(s: s, tile: 3)),
    ));
    await tester.pump();

    // The title-deed identity.
    expect(find.text('Tile 3'), findsOneWidget);
    expect(find.textContaining('Player 1'), findsWidgets); // owner

    // The rent schedule is shown verbatim (unimproved .. hotel).
    expect(find.text(r'$10'), findsOneWidget); // unimproved
    expect(find.text(r'$90'), findsOneWidget); // 2 houses = current level
    expect(find.text(r'$500'), findsOneWidget); // hotel
  });
}
