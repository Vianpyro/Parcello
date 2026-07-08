// Wire-format compatibility checks against the server's serde output
// (snake_case, type-tagged). If these fail, the protocol drifted.

import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/main.dart';
import 'package:parcello_client/protocol.dart';
import 'package:parcello_client/session.dart';

const sampleView = {
  'phase': {'type': 'active'},
  'players': [
    {
      'id': 'guest:alice',
      'name': 'Alice',
      'cash': 1450,
      'position': 3,
      'in_jail': false,
      'jail_cards': 1,
      'bankrupt': false,
    },
    {
      'id': 'guest:bob',
      'name': 'Bob',
      'cash': 1500,
      'position': 0,
      'in_jail': true,
      'jail_cards': 0,
      'bankrupt': false,
    },
  ],
  'current': 1,
  'turn': {
    'type': 'blind_auction',
    'tile': 3,
    'bids': [null, 60],
  },
  'tiles': [
    {'owner': null, 'houses': 0, 'mortgaged': false},
    {'owner': 0, 'houses': 2, 'mortgaged': true},
  ],
  'turn_count': 7,
  'pending_trades': [
    {
      'id': 4,
      'from': 0,
      'to': 1,
      'give_cash': 50,
      'give_tiles': [1],
      'receive_cash': 0,
      'receive_tiles': <int>[],
    },
  ],
};

void main() {
  test('ClientView parses the server view shape', () {
    final v = ClientView.fromJson(sampleView);
    expect(v.finished, false);
    expect(v.players[0].jailCards, 1);
    expect(v.players[1].inJail, true);
    expect(v.current, 1);
    expect(v.turn.type, 'blind_auction');
    expect(v.turn.tile, 3);
    expect(v.turn.bids, [null, 60]);
    expect(v.tiles[1].owner, 0);
    expect(v.tiles[1].mortgaged, true);
    expect(v.pendingTrades.single.giveTiles, [1]);
  });

  test('ClientView tolerates pre-trade and pre-jail-card states', () {
    // `pending_trades` and `jail_cards` are serde-defaulted server-side;
    // an older snapshot may omit them entirely.
    final old = Map<String, dynamic>.from(sampleView)
      ..remove('pending_trades')
      ..['phase'] = {'type': 'finished', 'winner': 0}
      ..['players'] = [
        {
          'id': 'guest:alice',
          'name': 'Alice',
          'cash': 0,
          'position': 0,
          'in_jail': false,
          'bankrupt': true,
        },
      ];
    final v = ClientView.fromJson(old);
    expect(v.finished, true);
    expect(v.winner, 0);
    expect(v.players.single.jailCards, 0);
    expect(v.pendingTrades, isEmpty);
  });

  test('TileDef parses property, tax, and corner kinds', () {
    final property = TileDef.fromJson({
      'id': 'ave_a',
      'name': 'Ave A',
      'kind': {
        'type': 'property',
        'group': 'brown',
        'price': 60,
        'house_cost': 50,
        'rents': [2, 10, 30, 90, 160, 250],
        'rent_model': 'houses',
      },
    });
    expect(property.isProperty, true);
    expect(property.price, 60);
    expect(property.group, 'brown');

    final tax = TileDef.fromJson({
      'id': 'tax',
      'name': 'City Tax',
      'kind': {'type': 'tax', 'amount': 100},
    });
    expect(tax.isProperty, false);
    expect(tax.amount, 100);

    final go = TileDef.fromJson({
      'id': 'go',
      'name': 'Go',
      'kind': {'type': 'go'},
    });
    expect(go.kind, 'go');
    expect(go.rentModel, 'houses');
  });

  test('describeEvent covers jail cards and falls back on unknown types', () {
    String p(int i) => 'P$i';
    String t(int i) => 'T$i';
    expect(
      describeEvent({'type': 'jail_card_received', 'player': 0}, p, t),
      'P0 received a get-out-of-jail-free card',
    );
    expect(
      describeEvent({'type': 'dice_rolled', 'player': 1, 'd1': 3, 'd2': 4}, p, t),
      'P1 rolled 3+4 = 7',
    );
    expect(
      describeEvent({'type': 'brand_new_event', 'x': 1}, p, t),
      contains('brand_new_event'),
    );
  });

  testWidgets('connect screen renders and requires a name', (tester) async {
    await tester.pumpWidget(ParcelloApp(session: GameSession()));
    expect(find.text('Connect'), findsOneWidget);
    expect(find.text('Display name'), findsOneWidget);
    // Tapping without a name must not navigate or crash.
    await tester.tap(find.text('Connect'));
    await tester.pump();
    expect(find.text('Connect'), findsOneWidget);
  });
}
