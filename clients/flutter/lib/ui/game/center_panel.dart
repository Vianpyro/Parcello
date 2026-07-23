/// The HUD on the board's sage plaza: status line, clocks, the market and
/// pool lines, the VP legend, the action buttons, and the event log.
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../../typography.dart';
import 'actions_panel.dart';
import 'countdown.dart';
import 'event_log.dart';
import 'toggles.dart';

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
          Row(children: [
            // The wordmark yields first when the board's centre gets tight:
            // the clocks and toggles beside it are functional, it is not.
            const Flexible(
              child: Text('PARCELLO',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontSize: 20,
                      fontWeight: FontWeight.bold,
                      letterSpacing: 3,
                      color: Pc.gold)),
            ),
            const Spacer(),
            // Shown for the whole game, end included: the final time left is
            // part of the result (a bankruptcy win keeps time on the clock).
            if (s.gameEndsAt != null) ...[
              Countdown(endsAt: s.gameEndsAt!),
              const SizedBox(width: Pc.s8),
            ],
            MotionButton(s: s),
            const MuteButton(),
          ]),
        const SizedBox(height: Pc.s4),
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
          if (_poolsLine(t) != null) ...[
            const SizedBox(height: Pc.s2),
            Text(_poolsLine(t)!,
                style: PcText.caption),
          ],
          if (_forecastLine(t) != null) ...[
            const SizedBox(height: Pc.s2),
            Text(_forecastLine(t)!,
                style: PcText.caption),
          ],
          if (_spotlightLine(t) != null) ...[
            const SizedBox(height: Pc.s2),
            Text(_spotlightLine(t)!,
                style: PcText.caption),
          ],
          if (_vpLegend(t) != null) ...[
            const SizedBox(height: Pc.s6),
            _vpLegend(t)!,
          ],
          const SizedBox(height: Pc.s6),
          ActionsPanel(s: s),
          const SizedBox(height: Pc.s6),
          Expanded(child: EventLog(log: s.log)),
        ]),
      ),
    );
  }

  /// How victory points are earned (ADR-0020), front and center on the
  /// table - the race is the win condition but its scoring was opaque in
  /// playtests (2026-07). Null when the VP race is off.
  Widget? _vpLegend(AppLocalizations t) {
    final target = s.content?.winVictoryPoints ?? 0;
    if (s.view == null || target <= 0) return null;
    final rows = [
      ('1', t.vpLegendUtilityTile),
      ('2', t.vpLegendMaxedTile),
      ('3', t.vpLegendFullGroup),
      ('+2', t.vpLegendRoundBonus),
    ];
    return Container(
      padding: const EdgeInsets.all(Pc.s8),
      decoration: BoxDecoration(
        color: Pc.goldWash,
        borderRadius: Pc.radius,
        border: Border.all(color: Pc.gold, width: 1),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(t.vpLegendHeader(target),
            style: const TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.bold,
                color: Pc.goldDark,
                letterSpacing: 1)),
        const SizedBox(height: 3),
        for (final (pts, what) in rows)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 1),
            child: Row(children: [
              SizedBox(
                width: Pc.s24,
                child: Text(pts,
                    style: const TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.bold,
                        color: Pc.goldDark)),
              ),
              Expanded(
                child: Text(what,
                    style: PcText.caption),
              ),
            ]),
          ),
        ..._roundProgress(t),
      ]),
    );
  }

  /// Live state of the round metronome (ADR-0020), so the `+2` above stops
  /// looking like it arrives out of nowhere: a "round" completes when every
  /// surviving player has cycled a full hand of movement cards, and the
  /// bonus banks to whoever is richest at that instant. The round number is
  /// the MINIMUM hands-cycled across survivors - so progress is simply how
  /// many players have already pulled ahead of that minimum.
  List<Widget> _roundProgress(AppLocalizations t) {
    final v = s.view;
    if (v == null || v.finished) return const [];
    final alive = [
      for (var i = 0; i < v.players.length; i++)
        if (!v.players[i].bankrupt) i,
    ];
    if (alive.isEmpty) return const [];
    final round =
        alive.map((i) => v.players[i].handsCycled).reduce((a, b) => a < b ? a : b);
    final done = alive.where((i) => v.players[i].handsCycled > round).toList();
    // Whoever would bank the +2 if the round closed right now: strictly
    // richest, ties to the lowest seat (mirrors `award_round_bonus`).
    var leader = alive.first;
    for (final i in alive) {
      if (v.players[i].cash > v.players[leader].cash) leader = i;
    }
    return [
      const SizedBox(height: Pc.s6),
      const Divider(height: 1, color: Pc.hairlineGold),
      const SizedBox(height: 5),
      Row(children: [
        Text(t.roundLabel(round + 1),
            style: const TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.bold,
                color: Pc.goldDark,
                letterSpacing: 1)),
        const SizedBox(width: Pc.s8),
        // One pip per surviving player: filled once they have cycled their
        // hand for this round. All filled = the bonus fires.
        for (final i in alive)
          Container(
            width: 10,
            height: 10,
            margin: const EdgeInsets.only(right: 3),
            decoration: BoxDecoration(
              color: done.contains(i)
                  ? pawnColor(i)
                  : Colors.transparent,
              shape: BoxShape.circle,
              border: Border.all(
                  color: pawnColor(i), width: 1.5),
            ),
          ),
        const SizedBox(width: Pc.s4),
        // The pips already say who the table waits on; the count is the part
        // that can be clipped when the centre is tight.
        Flexible(
          child: Text(t.roundHandsCycled(done.length, alive.length),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: PcText.whisper),
        ),
      ]),
      const SizedBox(height: Pc.s2),
      Text(
        t.roundBonusHint(s.playerName(leader)),
        style: PcText.whisper.copyWith(color: Pc.textMuted),
      ),
    ];
  }

  /// Shared building pools (ADR-0019): "the tension only works if everyone
  /// watches the shelf empty." Null when pooling is off entirely.
  String? _poolsLine(AppLocalizations t) {
    final v = s.view;
    if (v == null) return null;
    final subs = v.subsidiariesAvailable;
    final congs = v.conglomeratesAvailable;
    if (subs == null && congs == null) return null;
    return t.poolsLine(
        subs?.toString() ?? t.poolsUnlimited, congs?.toString() ?? t.poolsUnlimited);
  }

  /// Public market forecast (ADR-0021): reveals draws already made, not the
  /// generator. Null when nothing is scheduled or active.
  String? _forecastLine(AppLocalizations t) {
    final v = s.view;
    final c = s.content;
    if (v == null || c == null) return null;
    final f = v.forecast;
    if (f.active == null && f.queue.isEmpty) return null;
    final parts = <String>[];
    if (f.active != null) {
      final a = f.active!;
      final sign = a.magnitudePct > 0 ? '+' : '';
      parts.add(t.forecastActive(
          c.marketEventName(a.eventId), '$sign${a.magnitudePct}', a.endsAtTurn));
    }
    if (f.queue.isNotEmpty) {
      final upcoming = f.queue
          .map((e) =>
              t.forecastUpcomingItem(c.marketEventName(e.eventId), e.startsAtTurn))
          .join(', ');
      parts.add(t.forecastUpcoming(upcoming));
    }
    return parts.join(' | ');
  }

  /// The Exposition corner's spotlight (ADR-0026): fully public, no per-seat
  /// masking. Null when nothing is currently spotlit.
  String? _spotlightLine(AppLocalizations t) {
    final v = s.view;
    final c = s.content;
    final sp = v?.spotlight;
    if (v == null || c == null || sp == null) return null;
    // Prefer the live room rules (host may have tweaked them, ADR-0015);
    // fall back to the content snapshot from join. A permanent spotlight
    // carries u32::MAX as its expiry sentinel - don't print that.
    final pct = s.settings?.rules.spotlightRentPct ?? c.spotlightRentPct;
    final until = sp.expiresAtTurn >= 0xFFFFFFFF
        ? t.spotlightUntilReplaced
        : t.spotlightEndsTurn(sp.expiresAtTurn);
    return t.spotlightLine(c.board[sp.tile].name, pct, until);
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
