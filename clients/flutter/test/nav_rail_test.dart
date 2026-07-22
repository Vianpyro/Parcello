/// The left nav rail (DDR-0021) shows its three real entries and opens real
/// content - here, the Objectives sheet with the VP scoring. Guards the rail
/// wiring (no dead/placeholder buttons).
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/l10n/app_localizations.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';
import 'package:parcello_client/ui/game/nav_rail.dart';

GameSession _room() => GameSession()
  ..content = GameContent.fromJson({
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
  });

void main() {
  testWidgets('nav rail shows its entries and opens the objectives sheet',
      (tester) async {
    final s = _room();
    await tester.pumpWidget(MaterialApp(
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(body: NavRail(s: s)),
    ));
    await tester.pump();

    // The three real entries (Chat is intentionally absent - no backend).
    expect(find.text('Menu'), findsOneWidget);
    expect(find.text('Objectives'), findsOneWidget);
    expect(find.text('History'), findsOneWidget);

    // Opening Objectives shows the real VP scoring (the +2 round bonus marker).
    await tester.tap(find.text('Objectives'));
    await tester.pumpAndSettle();
    expect(find.text('+2'), findsOneWidget);
  });
}
