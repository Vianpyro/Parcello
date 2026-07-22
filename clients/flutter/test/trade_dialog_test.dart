/// The trade composer must not crash when its selected recipient disappears.
///
/// A recipient can go bankrupt or leave while the composer dialog is open (an
/// async offer, ADR-0007 - others act on their own turns). The recipient then
/// drops out of the DropdownButton's candidate list, and a stale selection made
/// the button's value match no item - a hard Flutter assertion. `TradeDialog`
/// now re-points the selection at a still-valid opponent on every build.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/side/trade_dialog.dart';

Map<String, dynamic> _content() => {
      'mods': [
        {'id': 'base'}
      ],
      'content': {
        'board': [
          for (var i = 0; i < 6; i++)
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

// Seat 0 (me) owns t1, seat 1 owns t2, seat 2 owns t3 - so both the "give" and
// "want" tile lists have a checkbox to interact with.
Map<String, dynamic> _view({required bool p1Bankrupt}) => {
      'phase': {'type': 'active'},
      'players': [
        for (var p = 0; p < 3; p++)
          {
            'id': 'guest:$p',
            'name': 'Player $p',
            'cash': 1500,
            'position': 0,
            'in_jail': false,
            'bankrupt': p == 1 && p1Bankrupt,
            'hand': [2, 3],
            'victory_points': 0,
            'hands_cycled': 0,
          },
      ],
      'current': 0,
      'turn': {'type': 'await_move'},
      'tiles': [
        for (var i = 0; i < 6; i++)
          {
            'owner': i == 1
                ? 0
                : i == 2
                    ? 1
                    : i == 3
                        ? 2
                        : null,
            'houses': 0,
            'mortgaged': false,
          },
      ],
      'pending_trades': <dynamic>[],
    };

GameSession _room({required bool p1Bankrupt}) => GameSession()
  ..content = GameContent.fromJson(_content())
  ..view = ClientView.fromJson(_view(p1Bankrupt: p1Bankrupt))
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
  testWidgets('trade composer survives its selected recipient going bankrupt',
      (tester) async {
    final s = _room(p1Bankrupt: false);
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(body: TradeDialog(s: s)),
    ));
    await tester.pump();

    // Player 1 is the default recipient; the composer has give/want checkboxes.
    expect(find.byType(CheckboxListTile), findsWidgets);

    // Player 1 bankrupts while the composer is open, then the player interacts
    // (toggles a tile) - which rebuilds the dialog with the stale selection.
    // Before the fix this asserted (dropdown value no longer among items).
    s.view = ClientView.fromJson(_view(p1Bankrupt: true));
    await tester.tap(find.byType(CheckboxListTile).first);
    await tester.pump();

    expect(tester.takeException(), isNull);
    expect(find.byType(TradeDialog), findsOneWidget);
  });
}
