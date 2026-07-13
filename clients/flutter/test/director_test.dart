/// The animation director's compiler (ADR-0030).
///
/// `compile` is a pure function, which is the whole reason it was split out of
/// `GameSession`: the budget rule and the coalescing rule are checkable here,
/// with no socket, no clock and no render tree.
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/director.dart';
import 'package:parcello_client/motion.dart';
import 'package:parcello_client/stage.dart';

CompileCtx ctx({
  Map<int, int> positions = const {0: 0, 1: 0},
  MotionProfile profile = MotionProfile.full,
  int boardLen = 32, // mods/base
  int? mySeat = 0,
}) =>
    CompileCtx(
      boardLen: boardLen,
      jailTile: 8,
      mySeat: mySeat,
      positions: positions,
      tileName: (t) => 'tile $t',
      playerName: (s) => 'player $s',
      profile: profile,
    );

/// The worst realistic chain, and the one that motivated ADR-0030: a movement
/// card lands on a chance tile, the card teleports the player across the board
/// through Go, and they collect salary on the way. Under the old inline
/// director this cost ~6980ms against a 6s server cap.
List<Map<String, dynamic>> chanceChain() => [
      {'type': 'movement_card_played', 'player': 0, 'value': 6},
      {'type': 'moved', 'player': 0, 'from': 0, 'to': 6, 'passed_go': false},
      {
        'type': 'card_drawn',
        'player': 0,
        'deck': 'chance',
        'card': 'advance',
        'text': 'Advance to the Exposition.',
      },
      {'type': 'moved', 'player': 0, 'from': 6, 'to': 4, 'passed_go': true},
      {'type': 'salary_paid', 'player': 0, 'amount': 200},
    ];

void main() {
  group('budget (ADR-0030)', () {
    test('the chance -> teleport chain that used to blow the 6s cap now fits',
        () {
      final plan = compile(chanceChain(), ctx());
      expect(plan.cost, lessThanOrEqualTo(Motion.budget));
    });

    test('a 4-deep card chain - the engine\'s worst case - still fits', () {
      // MAX_CARD_CHAIN_DEPTH = 4: a card that sends you to a card tile, four
      // times over. Four full reveals alone would be 4.8s.
      final events = <Map<String, dynamic>>[
        {'type': 'movement_card_played', 'player': 0, 'value': 5},
        {'type': 'moved', 'player': 0, 'from': 0, 'to': 5, 'passed_go': false},
      ];
      var at = 5;
      for (var i = 0; i < 4; i++) {
        final to = (at + 7) % 32;
        events.add({
          'type': 'card_drawn',
          'player': 0,
          'deck': 'chance',
          'card': 'c$i',
          'text': 'Card $i sends you onward.',
        });
        events.add(
            {'type': 'moved', 'player': 0, 'from': at, 'to': to, 'passed_go': false});
        at = to;
      }
      final plan = compile(events, ctx());
      expect(plan.cost, lessThanOrEqualTo(Motion.budget));
    });

    test('every beat still applies when the plan is truncated', () {
      // Truncation zeroes a beat's duration; it never drops its apply(). The
      // pawn must still end up where the server says it does.
      final stage = StageState();
      final plan = compile(chanceChain(), ctx());
      for (final b in plan.beats) {
        b.apply(stage);
      }
      expect(stage.pawnAt[0], 4, reason: 'final position survives compression');
    });

    test('P1 is never compressed, even in an over-budget plan', () {
      final events = [
        ...chanceChain(),
        {'type': 'player_bankrupt', 'player': 1, 'creditor': 0},
      ];
      final plan = compile(events, ctx());
      final arrest = plan.beats.whereType<ArrestBeat>().single;
      expect(arrest.cost, Motion.arrest,
          reason: 'a bankruptcy is never shortened to make room for a card chain');
    });
  });

  group('motion profiles (ADR-0030)', () {
    test('instant costs nothing and still applies every state change', () {
      final plan =
          compile(chanceChain(), ctx(profile: MotionProfile.instant));
      expect(plan.cost, Duration.zero);

      final stage = StageState();
      for (final b in plan.beats) {
        b.apply(stage);
      }
      expect(stage.pawnAt[0], 4);
      expect(stage.chits, hasLength(1), reason: 'the salary is still shown');
    });

    test('reduced halves the plan', () {
      final full = compile([
        {'type': 'moved', 'player': 0, 'from': 0, 'to': 3, 'passed_go': false},
      ], ctx());
      final reduced = compile([
        {'type': 'moved', 'player': 0, 'from': 0, 'to': 3, 'passed_go': false},
      ], ctx(profile: MotionProfile.reduced));
      expect(reduced.cost.inMilliseconds,
          closeTo(full.cost.inMilliseconds / 2, 2));
    });
  });

  group('coalescing', () {
    test('a bankruptcy portfolio is one band sweep, not eight', () {
      final events = <Map<String, dynamic>>[
        {'type': 'player_bankrupt', 'player': 1, 'creditor': 0},
        for (final t in [3, 5, 6, 8, 11, 13, 14, 16])
          {'type': 'property_transferred', 'tile': t, 'from': 1, 'to': 0},
      ];
      final plan = compile(events, ctx());
      final sweeps = plan.beats.whereType<BandSweepBeat>();
      expect(sweeps, hasLength(1));
      expect(sweeps.single.tiles, hasLength(8));
      // An 18-tile bankruptcy and a 2-tile one take about the same time: the
      // information is the same, only how much of the board changes differs.
      expect(sweeps.single.cost.inMilliseconds, lessThan(800));
    });

    test('a property returning to the bank sweeps to no owner', () {
      final plan = compile([
        {'type': 'property_transferred', 'tile': 7, 'from': 1, 'to': null},
      ], ctx());
      expect(plan.beats.whereType<BandSweepBeat>().single.tiles[7], -1);
    });
  });

  group('the money rule', () {
    test('rent is ONE chit travelling from the payer to the owner', () {
      // The old client floated only the payer's loss: the owner - who just
      // earned the game's core income - saw nothing at all.
      final plan = compile([
        {'type': 'rent_paid', 'from': 1, 'to': 0, 'tile': 6, 'amount': 120},
      ], ctx(positions: {0: 12, 1: 6}));

      final chit = plan.beats.whereType<ChitBeat>().single;
      expect(chit.from, isA<TileAnchor>()); // the payer's pawn
      expect((chit.from as TileAnchor).tile, 6);
      expect(chit.to, isA<SeatAnchor>()); // the owner's marker
      expect((chit.to as SeatAnchor).seat, 0);
    });

    test('the same rent reads as a gain, a loss, or neither - by seat', () {
      // Money is typed per observer, not per event. One payment, three
      // readings: this is what makes "who paid whom" free, and what keeps an
      // attack on you from ever being ambient.
      ChitBeat rentSeenBy(int? seat) => compile([
            {'type': 'rent_paid', 'from': 1, 'to': 0, 'tile': 6, 'amount': 120},
          ], ctx(positions: {0: 12, 1: 6}, mySeat: seat))
              .beats
              .whereType<ChitBeat>()
              .single;

      expect(rentSeenBy(0).kind, ChitKind.gain); // the owner earns
      expect(rentSeenBy(0).text, r'+$120');
      expect(rentSeenBy(1).kind, ChitKind.loss); // the payer pays
      expect(rentSeenBy(1).text, r'-$120');
      expect(rentSeenBy(2).kind, ChitKind.neutral); // the table watches
      expect(rentSeenBy(2).text, r'$120');
    });

    test('a sprung boost trap amplifies the rent it inflated', () {
      // The trap was armed turns ago and, until now, sprang silently: the
      // victim saw a large number and no reason for it. The chit grows as it
      // crosses the tile - that growth IS the explanation.
      final plan = compile([
        {'type': 'rent_paid', 'from': 1, 'to': 0, 'tile': 6, 'amount': 480},
        {'type': 'rent_boost_consumed', 'tile': 6},
      ], ctx(positions: {0: 12, 1: 6}, mySeat: 1));

      expect(plan.beats.whereType<ChitBeat>().single.amplified, isTrue);
      expect(plan.beats.whereType<ThreatBeat>().single.tile, 6);
    });

    test('rent chits ride the concurrent lane - money never blocks the game',
        () {
      final plan = compile([
        {'type': 'rent_paid', 'from': 1, 'to': 0, 'tile': 6, 'amount': 120},
      ], ctx());
      expect(plan.beats.single.lane, Lane.concurrent);
    });
  });

  group('the sealed bid (ADR-0018), the game\'s core loop', () {
    test('opening the window recedes the board and lifts the tile', () {
      final plan = compile([
        {
          'type': 'blind_auction_opened',
          'tile': 6,
          'discoverer': 0,
          'floor': 140,
        },
      ], ctx());
      final focus = plan.beats.whereType<FocusBeat>().single;
      expect(focus.tier, Tier.decide);
      expect(focus.tile, 6);
      expect(focus.recede, isTrue);
    });

    test('resolution reveals every bid, then pays, then takes the band', () {
      final plan = compile([
        {
          'type': 'blind_auction_resolved',
          'tile': 6,
          'discoverer': 0,
          'winner': 1,
          'amount': 180,
          'bids': [140, 200, 0],
        },
      ], ctx());
      expect(plan.beats, hasLength(3));
      final reveal = plan.beats.whereType<BidRevealBeat>().single.reveal;
      expect(reveal.bids, [140, 200, 0]);
      expect(reveal.winner, 1);
      // Won above the floor after a contest: the 90% discount shows as the
      // chit shrinking on its way to the tile.
      expect(reveal.discounted, isTrue);
      expect(plan.beats.whereType<BandSweepBeat>().single.tiles, {6: 1});
    });

    test('an unsold tile reveals the bids and transfers nothing', () {
      final plan = compile([
        {
          'type': 'blind_auction_resolved',
          'tile': 6,
          'discoverer': 0,
          'winner': null,
          'amount': 0,
          'bids': [0, 0],
        },
      ], ctx());
      expect(plan.beats.whereType<BidRevealBeat>(), hasLength(1));
      expect(plan.beats.whereType<BandSweepBeat>(), isEmpty);
    });
  });

  group('the truth rule: motion may not imply a path the engine did not take',
      () {
    test('a wrap that collected salary hops the whole way (it crossed Go)', () {
      final beat = compile([
        {'type': 'moved', 'player': 0, 'from': 30, 'to': 4, 'passed_go': true},
      ], ctx()).beats.whereType<MoveBeat>().single;
      expect(beat.forceHop, isTrue);
      expect(beat.straight, isFalse);
    });

    test('a wrap that did NOT collect salary glides straight (do not pass Go)',
        () {
      final beat = compile([
        {'type': 'moved', 'player': 0, 'from': 30, 'to': 4, 'passed_go': false},
      ], ctx()).beats.whereType<MoveBeat>().single;
      expect(beat.straight, isTrue,
          reason: 'hopping would promise a salary that never comes');
      expect(beat.forceHop, isFalse);
    });

    test('a plain forward move hops tile by tile - the count is the information',
        () {
      final beat = compile([
        {'type': 'moved', 'player': 0, 'from': 2, 'to': 7, 'passed_go': false},
      ], ctx()).beats.whereType<MoveBeat>().single;
      expect(beat.straight, isFalse);
      expect(beat.cost, greaterThan(Motion.hop(5)));
    });
  });

  group('P4 is never a beat', () {
    test('ambient events compile to nothing', () {
      final plan = compile([
        {'type': 'blind_bid_submitted', 'player': 1},
        {'type': 'bribe_vote_cast', 'player': 2},
        {'type': 'trade_declined', 'trade': 1, 'from': 0, 'to': 1},
        {'type': 'spotlight_ended', 'tile': 4},
        {'type': 'market_event_expired', 'event_id': 'bubble'},
        {'type': 'jail_card_received', 'player': 0},
        {'type': 'turn_started', 'player': 1},
      ], ctx());
      expect(plan.beats, isEmpty);
      expect(plan.cost, Duration.zero);
    });
  });
}
