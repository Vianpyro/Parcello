/// The half-typed bid must survive an animation frame.
///
/// This is a real bug that was already lived through, and the reason for the
/// shape of `GameScreen.build`: the action panel is built ONCE there and handed
/// to the board as a `child`, so a stage repaint reuses the same element - text
/// fields and all. Sharing one notifier between transient visual state and
/// durable input state is what used to wipe the bid out from under the player.
///
/// It guards a structure, not a pixel: any refactor that rebuilds the centre
/// panel per animation frame fails here.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/ui/game/game_screen.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';

/// A 32-tile ring, mid sealed-bid window on tile 3 - the one phase that puts a
/// text field in the board's centre.
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
      'current': 1, // someone else discovered it; seat 0 is a plain bidder
      'turn': {
        'type': 'blind_auction',
        'tile': 3,
        'bids': [null, null],
      },
      'tiles': [
        for (var i = 0; i < 32; i++)
          {'owner': null, 'houses': 0, 'mortgaged': false},
      ],
      'pending_trades': <dynamic>[],
    };

GameSession _biddingRoom() => GameSession()
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
  testWidgets('a half-typed bid survives an animation frame', (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final s = _biddingRoom();
    // Mirrors ParcelloApp: the screen is rebuilt from the session's notifier.
    // That is the path that matters - a session notification (an animation
    // beat, another seat's bid landing, a hover) rebuilds the whole screen,
    // and it is where the bid used to get wiped.
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: ListenableBuilder(
        listenable: s,
        builder: (_, _) => GameScreen(s: s),
      ),
    ));
    await tester.pump(const Duration(milliseconds: 400));

    // The window seeds the field at the list price; the player is mid-edit.
    final field = find.byType(TextField);
    expect(field, findsOneWidget, reason: 'the sealed-bid window has a field');
    await tester.enterText(field, '137');
    await tester.pump();
    expect(find.text('137'), findsOneWidget);

    // The session notifies - hovering a tile, a beat landing, another seat's
    // bid arriving - and the screen rebuilds under the player's fingers.
    s.setHoverTile(5);
    await tester.pump();
    expect(find.text('137'), findsOneWidget,
        reason: 'a session rebuild must not reseed the bid field');

    // A burst of them, the way a played chain arrives.
    for (var i = 0; i < 5; i++) {
      s.setHoverTile(i.isEven ? 7 : null);
      await tester.pump();
    }
    expect(find.text('137'), findsOneWidget,
        reason: 'still there after a burst of rebuilds');

    // The stage repainting (a chit in flight) must not touch it either.
    s.stage.bump();
    await tester.pump();
    expect(find.text('137'), findsOneWidget);
  });

  testWidgets('the sealed-bid field submits on Enter / the Deck Done key',
      (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final s = _biddingRoom();
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: ListenableBuilder(
        listenable: s,
        builder: (_, _) => GameScreen(s: s),
      ),
    ));
    await tester.pump(const Duration(milliseconds: 400));

    // The window is only 12s: the field must submit without the player having
    // to leave it for the Bid button, and the on-screen keyboard must offer a
    // submit key rather than a newline.
    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.onSubmitted, isNotNull,
        reason: 'Enter / the OSK Done key must send the bid');
    expect(field.textInputAction, TextInputAction.done);
  });
}
