/// The board's sage-plaza centre, reduced to the single contextual decision
/// (game-screen refonte step 1, DDR-0021): the turn prompt, the clocks strictly
/// tied to the decision in flight (per-turn / time-bank / bid / vote windows),
/// and the contextual action panel. The game clock moved to the player bar; the
/// market lines to `MarketStrip` under it; the VP legend duplicates NavRail's
/// Objectives and the round metronome the player-bar pips; the event log
/// duplicates NavRail's History - all removed from here.
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../../typography.dart';
import 'actions_panel.dart';
import 'countdown.dart';

class CenterPanel extends StatelessWidget {
  final GameSession s;
  const CenterPanel({super.key, required this.s});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    // A dark plate on the sage plaza: the HUD is a panel *on* the board, not a
    // hole in it. (The plaza itself stays sage - `docs/visual-identity.md`.)
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: Pc.surface,
        borderRadius: Pc.radius,
        border: Border.all(color: Pc.goldDark, width: 1.5),
      ),
      child: DefaultTextStyle(
        style: PcText.body,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          // The turn prompt and the clocks tied to the decision in flight share
          // one line; the game clock is no longer here (it moved to the player
          // bar), so this is the whole "what am I deciding, and how long do I
          // have" header.
          Row(children: [
          Expanded(child: _turnPrompt(t)),
          if (s.turnEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: Pc.s6),
            Countdown(
                endsAt: s.turnEndsAt!,
                icon: Icons.hourglass_bottom,
                warnSecs: 10,
                // The server's own clock only starts once this seat's
                // render ack lands (ADR-0028) - the display must not look
                // like movement/animation is eating thinking time.
                paused: s.isAnimating),
          ],
          // Personal time bank (ADR-0023): a flat reserve for the whole
          // plain turn window, then counts down to the hard stop. Never
          // refilled.
          if (s.bankEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: Pc.s6),
            Countdown(
                endsAt: s.bankEndsAt!,
                holdUntil: s.turnEndsAt,
                icon: Icons.account_balance,
                warnSecs: 10,
                paused: s.isAnimating),
          ],
          // Sealed-bid window (ADR-0018): a one-shot ~12s countdown, local
          // estimate only - the server alone decides when it actually
          // closes, and its clock waits for the whole table's acks
          // (ADR-0028).
          if (s.bidEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: Pc.s6),
            Countdown(
                endsAt: s.bidEndsAt!,
                icon: Icons.gavel,
                warnSecs: 3,
                paused: s.isAnimating),
          ],
          // Corruption bribe vote window (ADR-0024): same pattern.
          if (s.voteEndsAt != null && s.view?.finished == false) ...[
            const SizedBox(width: Pc.s6),
            Countdown(
                endsAt: s.voteEndsAt!,
                icon: Icons.how_to_vote,
                warnSecs: 2,
                paused: s.isAnimating),
          ],
        ]),
          const SizedBox(height: Pc.s6),
          ActionsPanel(s: s),
        ]),
      ),
    );
  }

  /// The turn prompt (game-screen refonte, DDR-0021): a prominent "your turn"
  /// headline when the table is waiting on ME, the plain status otherwise. The
  /// text is still `_status` for auctions/votes (it names who it waits on), only
  /// promoted to gold when I am one of the seats that must act.
  Widget _turnPrompt(AppLocalizations t) {
    final v = s.view;
    final phase = v?.turn.type;
    final everyoneActs = phase == 'blind_auction' || phase == 'bribe_vote';
    final mine = v != null && !v.finished && (s.myTurn || everyoneActs);
    final text =
        (mine && s.myTurn && !everyoneActs) ? t.statusYourTurn : _status(t);
    return Row(children: [
      if (mine) ...[
        const Icon(Icons.play_circle_fill, size: 16, color: Pc.gold),
        const SizedBox(width: Pc.s6),
      ],
      Expanded(
        child: Text(text,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: PcText.rowTitle.copyWith(
                color: mine ? Pc.gold : Pc.text,
                fontWeight: mine ? FontWeight.w800 : FontWeight.w600)),
      ),
    ]);
  }

  String _status(AppLocalizations t) {
    final v = s.view;
    if (v == null) {
      return s.seats.length >= 2
          ? t.statusReadyHostCanStart
          : t.statusWaitingForPlayers;
    }
    if (v.finished) return t.statusGameOver(s.playerName(v.winner!));
    final turn = v.turn;
    switch (turn.type) {
      case 'blind_auction':
        final pending = <int>[
          for (var i = 0; i < turn.bids.length; i++)
            if (turn.bids[i] == null) i
        ];
        final waiting = pending.isEmpty
            ? t.statusNobody
            : pending.map(s.playerName).join(', ');
        return t.statusSealedBid(s.tileName(turn.tile!), waiting);
      case 'bribe_vote':
        final pending = <int>[
          for (var i = 0; i < turn.votes.length; i++)
            if (i != turn.briber && turn.votes[i] == null) i
        ];
        final waiting = pending.isEmpty
            ? t.statusNobody
            : pending.map(s.playerName).join(', ');
        return t.statusBribeVote(
            s.playerName(turn.briber!), turn.amount!, waiting);
      default:
        return t.statusPlayerTurn(s.playerName(v.current));
    }
  }
}
