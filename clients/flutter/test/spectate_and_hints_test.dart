/// Spectator mode (ADR-0035) and the first-game coach marks: the badge and
/// the hint are both non-modal overlays on the game screen, so rendering
/// the screen in each state IS the assertion surface.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/coach_mark.dart';
import 'package:parcello_client/ui/game/game_screen.dart';

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
            'position': p,
            'in_jail': false,
            'bankrupt': false,
            'hand': [2, 3],
            'victory_points': 0,
            'hands_cycled': 0,
          },
      ],
      'current': 0,
      'turn': {'type': 'await_move', 'bids': <dynamic>[]},
      'tiles': [
        for (var i = 0; i < 32; i++)
          {'owner': null, 'houses': 0, 'mortgaged': false},
      ],
      'pending_trades': <dynamic>[],
    };

Widget _app(GameSession s) => MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: GameScreen(s: s),
    );

void main() {
  testWidgets('my turn shows the hand coach mark; dismissing hides it',
      (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final s = GameSession()
      ..resetHints()
      ..content = GameContent.fromJson(_content())
      ..view = ClientView.fromJson(_view())
      ..seat = 0;
    addTearDown(s.resetHints); // never leak dismissals into other tests

    await tester.pumpWidget(_app(s));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(CoachMark), findsOneWidget,
        reason: 'first await_move on my turn must coach the hand');

    await tester.tap(find.text('Got it'));
    await tester.pumpWidget(_app(s)); // rebuild with the dismissal applied
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byType(CoachMark), findsNothing,
        reason: 'a dismissed hint never returns');
    expect(s.hintSeen('hand'), isTrue);
  });

  testWidgets('spectating shows the badge and coaches nothing',
      (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final s = GameSession()
      ..resetHints()
      ..content = GameContent.fromJson(_content())
      ..view = ClientView.fromJson(_view())
      ..seat = null
      ..spectating = true;
    addTearDown(s.resetHints);

    await tester.pumpWidget(_app(s));
    await tester.pump(const Duration(milliseconds: 400));

    expect(find.textContaining('watching this game'), findsOneWidget,
        reason: 'the spectator badge names the mode (ADR-0035)');
    expect(find.byType(CoachMark), findsNothing,
        reason: 'spectators are not coached - the board is their tutorial');
    expect(activeHintId(s), isNull);
  });
}
