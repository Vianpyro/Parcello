/// The top player bar (game-screen refonte, DDR-0021): every seat as a compact
/// cell - pawn, name, cash, victory points, turn/rank state, the bid reveal -
/// laid out horizontally across the top. It reuses the ratified `SeatTile`
/// (CAR-0001) in its `compact` layout; the cross-seat domain logic (VP rank,
/// the round metronome, the status tags) lives here, exactly as it did when the
/// seats were a vertical list in the side panel (moved, not duplicated).
library;

import 'package:flutter/material.dart';

import '../../design/components/seat_tile.dart';
import '../../l10n/app_localizations.dart';
import '../../protocol.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../side/bid_chip.dart';
import 'countdown.dart';
import 'toggles.dart';

class PlayerBar extends StatelessWidget {
  final GameSession s;
  const PlayerBar({super.key, required this.s});

  /// VP leaderboard rank per seat (1 = leading), null for bankrupt seats or
  /// when the VP race is off. Ties break to the lowest seat, matching the
  /// engine. (Moved verbatim from the old side-panel seat list.)
  List<int?> _vpRanks(ClientView v) {
    final ranks = List<int?>.filled(v.players.length, null);
    if ((s.content?.winVictoryPoints ?? 0) <= 0) return ranks;
    final alive = [
      for (var i = 0; i < v.players.length; i++)
        if (!v.players[i].bankrupt) i,
    ]..sort((a, b) {
        final byVp = v.players[b].victoryPoints - v.players[a].victoryPoints;
        return byVp != 0 ? byVp : a - b;
      });
    for (var r = 0; r < alive.length; r++) {
      ranks[alive[r]] = r + 1;
    }
    return ranks;
  }

  @override
  Widget build(BuildContext context) {
    final v = s.view;
    final t = AppLocalizations.of(context);
    final count = v?.players.length ?? s.seats.length;
    final ranks = v != null ? _vpRanks(v) : List<int?>.filled(count, null);
    // Round metronome (ADR-0020): the round is the minimum hands-cycled across
    // survivors; anyone above it has already done their hand this round.
    final int? round = (v == null || v.finished)
        ? null
        : v.players
            .asMap()
            .entries
            .where((e) => !e.value.bankrupt)
            .map((e) => e.value.handsCycled)
            .fold<int?>(null, (m, h) => m == null || h < m ? h : m);
    final reveal = s.stage.bidReveal;
    final showVp = (s.content?.winVictoryPoints ?? 0) > 0;

    final cells = <Widget>[];
    for (var i = 0; i < count; i++) {
      final p = v?.players.elementAtOrNull(i);
      final seatInfo = s.seats.elementAtOrNull(i);
      final name = p?.name ?? seatInfo?.name ?? t.seatFallback(i);
      final isActive = v != null && !v.finished && v.current == i;
      final cycled =
          round != null && p != null && !p.bankrupt && p.handsCycled > round;
      final tags = [
        if (cycled) t.playerTagHandCycled,
        if (i == s.seat) t.playerTagYou,
        if (p?.inJail == true) t.playerTagJail,
        if (p?.jailRoute != null) t.playerTagRoute(p!.jailRoute!.join(',')),
        if ((p?.jailCards ?? 0) > 0) t.playerTagJailCard(p!.jailCards),
        if (seatInfo?.isBot == true)
          t.playerTagBot
        else if (seatInfo?.connected == false)
          t.playerTagOffline,
      ].join(' ');
      cells.add(Expanded(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Pc.s2),
          child: SeatTile(
            compact: true,
            seat: i,
            name: name,
            tags: tags,
            active: isActive,
            bankrupt: p?.bankrupt == true,
            rank: ranks[i],
            anchorKey: s.stage.anchors.seatKey(i),
            cash: p != null ? '\$${p.cash}' : null,
            netWorthLabel: p != null && s.gameEndsAt != null
                ? t.sideNetWorth(s.netWorth(i))
                : null,
            vpLabel: p != null && showVp
                ? t.sideVictoryPoints(p.victoryPoints, s.content!.winVictoryPoints)
                : null,
            trailingBid: reveal != null && i < reveal.bids.length
                ? BidChip(bid: reveal.bids[i], won: reveal.winner == i)
                : null,
          ),
        ),
      ));
    }

    // Top-right chrome cluster (DDR-0021: the game clock lives in the player
    // bar). The game clock is shown for the whole game, end included - the
    // final time left is part of the result (a bankruptcy win keeps time on the
    // clock). The motion/mute toggles moved here with it out of the emptied
    // board centre; they are global chrome, not part of the immediate decision.
    final trailing = <Widget>[
      if (s.gameEndsAt != null) ...[
        Countdown(endsAt: s.gameEndsAt!),
        const SizedBox(width: Pc.s8),
      ],
      MotionButton(s: s),
      const MuteButton(),
    ];

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: Pc.s8, vertical: Pc.s6),
      decoration: const BoxDecoration(
        color: Pc.surface2,
        border: Border(bottom: BorderSide(color: Pc.border)),
      ),
      child: Row(crossAxisAlignment: CrossAxisAlignment.center, children: [
        Expanded(
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: cells),
        ),
        const SizedBox(width: Pc.s8),
        Row(mainAxisSize: MainAxisSize.min, children: trailing),
      ]),
    );
  }
}
