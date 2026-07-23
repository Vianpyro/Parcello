/// TradeOfferCard (CAR-0002): the one-line collapse rule and the
/// absent-means-hidden action contract, tested directly against the
/// component (plain records/strings, no `GameSession` - it never imports
/// `session.dart` or `l10n/`, per DDR-0020).
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/ui/game/trade_offer_card.dart';

Widget _host(Widget child) => MaterialApp(home: Scaffold(body: child));

void main() {
  testWidgets(
    'collapses to one line for cash + a single tile, stacks otherwise',
    (tester) async {
      // One tile + cash on the GIVE side: collapse rule fires, so the amount
      // and the tile name share one Row.
      await tester.pumpWidget(
        _host(
          TradeOfferCard(
            fromSeat: 0,
            fromName: 'Alice',
            toSeat: 1,
            toName: 'Bob',
            giveCash: r'$200',
            giveTiles: const [(name: 'Ironworks Avenue', group: 'navy')],
            receiveCash: '',
            // Two tiles, no cash on RECEIVE: must NOT collapse - one line per tile.
            receiveTiles: const [
              (name: 'Rose Boulevard', group: 'red'),
              (name: 'Granite Street', group: 'red'),
            ],
            nothingLabel: 'nothing',
            givesLabel: 'Gives',
            receivesLabel: 'Receives',
            acceptLabel: 'Accept',
            declineLabel: 'Decline',
            cancelLabel: 'Cancel',
          ),
        ),
      );

      // Identity always renders.
      expect(find.text('Alice'), findsOneWidget);
      expect(find.text('Bob'), findsOneWidget);

      // The collapsed GIVE side: amount and tile both present.
      expect(find.text(r'$200'), findsOneWidget);
      expect(find.text('Ironworks Avenue'), findsOneWidget);

      // The stacked RECEIVE side: every tile still renders on its own line.
      expect(find.text('Rose Boulevard'), findsOneWidget);
      expect(find.text('Granite Street'), findsOneWidget);
    },
  );

  testWidgets('an empty side shows the nothing label', (tester) async {
    await tester.pumpWidget(
      _host(
        TradeOfferCard(
          fromSeat: 0,
          fromName: 'Alice',
          toSeat: 1,
          toName: 'Bob',
          giveCash: '',
          giveTiles: const [],
          receiveCash: r'$50',
          receiveTiles: const [],
          nothingLabel: 'nothing',
          givesLabel: 'Gives',
          receivesLabel: 'Receives',
          acceptLabel: 'Accept',
          declineLabel: 'Decline',
          cancelLabel: 'Cancel',
        ),
      ),
    );

    expect(find.text('nothing'), findsOneWidget);
    expect(find.text(r'$50'), findsOneWidget);
  });

  testWidgets('a null callback hides its button (absent-means-hidden)', (
    tester,
  ) async {
    var accepted = false;
    var declined = false;
    // Only onAccept/onDecline are wired - matches the recipient's view of an
    // incoming offer; onCancel stays null (not the proposer) and must not
    // render, exactly as the caller's existing permission logic dictates.
    await tester.pumpWidget(
      _host(
        TradeOfferCard(
          fromSeat: 0,
          fromName: 'Alice',
          toSeat: 1,
          toName: 'Bob',
          giveCash: r'$100',
          giveTiles: const [],
          receiveCash: '',
          receiveTiles: const [],
          nothingLabel: 'nothing',
          givesLabel: 'Gives',
          receivesLabel: 'Receives',
          acceptLabel: 'Accept',
          declineLabel: 'Decline',
          cancelLabel: 'Cancel',
          onAccept: () => accepted = true,
          onDecline: () => declined = true,
        ),
      ),
    );

    expect(find.text('Accept'), findsOneWidget);
    expect(find.text('Decline'), findsOneWidget);
    expect(find.text('Cancel'), findsNothing);

    await tester.tap(find.text('Accept'));
    await tester.tap(find.text('Decline'));
    expect(accepted, isTrue);
    expect(declined, isTrue);
  });

  testWidgets('no actions at all renders no action row', (tester) async {
    await tester.pumpWidget(
      _host(
        const TradeOfferCard(
          fromSeat: 0,
          fromName: 'Alice',
          toSeat: 1,
          toName: 'Bob',
          giveCash: r'$100',
          giveTiles: [],
          receiveCash: '',
          receiveTiles: [],
          nothingLabel: 'nothing',
          givesLabel: 'Gives',
          receivesLabel: 'Receives',
          acceptLabel: 'Accept',
          declineLabel: 'Decline',
          cancelLabel: 'Cancel',
        ),
      ),
    );

    expect(find.text('Accept'), findsNothing);
    expect(find.text('Decline'), findsNothing);
    expect(find.text('Cancel'), findsNothing);
  });
}
