/// The market strip (game-screen refonte step 1, DDR-0021): the public market
/// state - pools / forecast / spotlight - moved out of the board centre into a
/// thin band under the player bar. It must render NOTHING when there is no
/// market state, so it never steals board height at the 1024x600 floor
/// (SCREEN_ARCHITECTURE / layout_test); and it must show the band when there is.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/game/market_strip.dart';

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

Map<String, dynamic> _view({bool pools = false}) => {
      'phase': {'type': 'active'},
      'players': [
        {
          'id': 'guest:0',
          'name': 'Player 0',
          'cash': 1000,
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
        {'owner': null, 'houses': 0, 'mortgaged': false},
      ],
      'pending_trades': <dynamic>[],
      // Shared building pools (ADR-0019): present only in the `pools` case.
      if (pools) 'subsidiaries_available': 14,
      if (pools) 'conglomerates_available': 7,
    };

GameSession _room({required bool pools}) => GameSession()
  ..content = GameContent.fromJson(_content())
  ..view = ClientView.fromJson(_view(pools: pools))
  ..seat = 0;

Future<void> _pump(WidgetTester tester, GameSession s) => tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(body: MarketStrip(s: s)),
      ),
    );

void main() {
  testWidgets('renders nothing when there is no market state', (tester) async {
    await _pump(tester, _room(pools: false));
    await tester.pump();
    // No band means no Text at all: the strip collapses to SizedBox.shrink so
    // it takes zero board height at the floor.
    expect(find.byType(Text), findsNothing);
  });

  testWidgets('renders the band when a pool line is present', (tester) async {
    await _pump(tester, _room(pools: true));
    await tester.pump();
    expect(find.byType(Text), findsOneWidget);
    // The available counts are the information the strip carries.
    expect(find.textContaining('14'), findsOneWidget);
  });
}
