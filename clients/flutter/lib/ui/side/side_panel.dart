/// The right-hand column: seats, room, trades, and the end-of-game cards.
/// It scrolls because it grows with the room (six offers overflow a Deck).
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../protocol.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';
import '../coach_mark.dart';
import '../common.dart';
import 'bid_chip.dart';
import 'feedback_card.dart';
import 'settings_panel.dart';
import 'trade_dialog.dart';

class SidePanel extends StatelessWidget {
  final GameSession s;
  const SidePanel({super.key, required this.s});

  @override
  Widget build(BuildContext context) {
    final v = s.view;
    final t = AppLocalizations.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      // First-game coach mark (one at a time, never modal). In the side
      // panel, not floating over the board: here it participates in layout
      // (the panel scrolls, the layout tests size it) and can never cover
      // a tappable tile or the action panel.
      if (activeHintId(s) case final hint?)
        CoachMark(
          text: hintText(hint, t),
          onDismiss: () => s.dismissHint(hint),
        ),
      // Watching, not playing (ADR-0035): say so where the player card
      // normally promises agency.
      if (s.spectating)
        Card(
          color: Pc.surface2,
          child: Padding(
            padding: const EdgeInsets.all(10),
            child: Row(children: [
              const Icon(Icons.visibility_outlined, color: Pc.gold, size: 18),
              const SizedBox(width: Pc.s8),
              Expanded(
                child: Text(t.spectatingBadge,
                    style: PcText.label.copyWith(color: Pc.textMuted)),
              ),
              hoverSfx(TextButton(
                  onPressed: s.leaveRoom, child: Text(t.continueLabel))),
            ]),
          ),
        ),
      // Game over: replay together, or go back to the start screen.
      if (v != null && v.finished)
        Card(
          color: Pc.surface2,
          child: Padding(
            padding: const EdgeInsets.all(Pc.s12),
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(t.sideWinnerWins(s.playerName(v.winner!)),
                      style: const TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.bold,
                          color: Pc.gold)),
                  const SizedBox(height: Pc.s8),
                  Row(children: [
                    // Spectators cannot replay a game they never sat in
                    // (ADR-0035); they only get the way out.
                    if (!s.spectating) ...[
                      Expanded(child: wideButton(t.playAgain, s.sendPlayAgain)),
                      const SizedBox(width: Pc.s8),
                    ],
                    Expanded(
                        child: wideButton(t.continueLabel, s.leaveRoom,
                            primary: false)),
                  ]),
                  if (!s.spectating)
                    Text(t.playAgainHint,
                        style: PcText.caption),
                ]),
          ),
        ),
      Card(
        child: Padding(
          padding: const EdgeInsets.all(Pc.s12),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Expanded(
                child: Text(t.sideRoom(s.code ?? ''),
                    style: const TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.bold,
                        color: Pc.gold,
                        letterSpacing: 2)),
              ),
              if (s.code != null)
                hoverSfx(IconButton(
                  iconSize: 18,
                  visualDensity: VisualDensity.compact,
                  tooltip: t.copyRoomCode,
                  icon: const Icon(Icons.copy, color: Pc.textMuted),
                  onPressed: () => copyCode(context, s.code!),
                )),
            ]),
            const SizedBox(height: Pc.s6),
            // The seat list is the only part of the side panel the stage drives
            // (chit anchors, the sealed-bid reveal), so it is the only part that
            // repaints on an animation frame. The trade panel and the settings
            // fields below never do.
            ListenableBuilder(
                listenable: s.stage, builder: (context, _) => _players(t)),
            if (s.view == null) ...[
              const SizedBox(height: Pc.s8),
              wideButton(t.startGame,
                  s.seat == 0 && s.seats.length >= 2 ? s.sendStart : null),
              // Host-only bot controls. Bots fill empty seats but yield to
              // humans, so they never block a join (ADR-0014).
              if (s.seat == 0)
                Padding(
                  padding: const EdgeInsets.only(top: Pc.s6),
                  child: Row(children: [
                    Expanded(
                        child: wideButton(t.addBot,
                            s.seats.length < 6 ? s.addBot : null,
                            primary: false)),
                    const SizedBox(width: Pc.s6),
                    Expanded(
                        child: wideButton(t.removeBot,
                            s.seats.any((x) => x.isBot) ? s.removeBot : null,
                            primary: false)),
                  ]),
                ),
              if (s.code != null)
                Padding(
                  padding: const EdgeInsets.only(top: Pc.s6),
                  child: wideButton(t.copyCodeToShare, () => copyCode(context, s.code!),
                      primary: false),
                ),
              if (s.settings != null) SettingsPanel(s: s),
              // Cancel: leave the room (dissolves it for the host) and return
              // to the main menu. Keyboard/controller reachable like any button.
              const SizedBox(height: Pc.s6),
              wideButton(t.backToMenu, s.leaveRoom, primary: false),
            ],
          ]),
        ),
      ),
      Card(
          child: Padding(
              padding: const EdgeInsets.all(Pc.s12), child: _trades(context))),
      // Post-game survey: an ordinary side card, never a modal - it must
      // not block anything (no frustration by design).
      // The survey asks players about a game they played; a spectator's
      // submission would only bounce off the server (ADR-0035).
      if (s.view?.finished == true && !s.feedbackDone && !s.spectating)
        FeedbackCard(s: s),
      Card(
        child: Padding(
          padding: const EdgeInsets.all(Pc.s12),
          child: hoverSfx(OutlinedButton(
            style: OutlinedButton.styleFrom(
                foregroundColor: Pc.oxblood),
            onPressed: () async {
              final ok = await showDialog<bool>(
                context: context,
                builder: (ctx) => AlertDialog(
                  title: Text(t.resignConfirmTitle),
                  actions: [
                    hoverSfx(TextButton(
                        onPressed: () {
                          sfx.buttonNo();
                          Navigator.pop(ctx, false);
                        },
                        child: Text(t.cancel))),
                    hoverSfx(TextButton(
                        onPressed: () {
                          sfx.buttonYes();
                          Navigator.pop(ctx, true);
                        },
                        child: Text(t.resign))),
                  ],
                ),
              );
              if (ok == true) s.sendCmd({'type': 'resign'});
            },
            child: Text(t.resign),
          )),
        ),
      ),
    ]);
  }

  /// VP leaderboard rank per seat (1 = leading), null for bankrupt seats
  /// or when the VP race is off. Ties break to the lowest seat, matching
  /// every tiebreak in the engine.
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

  Widget _players(AppLocalizations t) {
    final v = s.view;
    final rows = <Widget>[];
    final count = v?.players.length ?? s.seats.length;
    final ranks = v != null ? _vpRanks(v) : List<int?>.filled(count, null);
    // Round metronome (ADR-0020): the round is the minimum hands-cycled
    // across survivors, so anyone above that minimum has already done their
    // hand this round - tag them so it is obvious who the table waits on.
    final int? round = (v == null || v.finished)
        ? null
        : v.players
            .asMap()
            .entries
            .where((e) => !e.value.bankrupt)
            .map((e) => e.value.handsCycled)
            .fold<int?>(null, (m, h) => m == null || h < m ? h : m);
    for (var i = 0; i < count; i++) {
      final p = v?.players.elementAtOrNull(i);
      final seatInfo = s.seats.elementAtOrNull(i);
      final name = p?.name ?? seatInfo?.name ?? t.seatFallback(i);
      // Whose turn is it: bold text alone read as too subtle in playtests
      // (2026-07) - a highlighted row + a leading marker reads at a glance.
      final isActive = v != null && !v.finished && v.current == i;
      final rank = ranks[i];
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
      rows.add(AnimatedContainer(
        duration: const Duration(milliseconds: 200),
        margin: const EdgeInsets.symmetric(vertical: Pc.s2),
        padding: EdgeInsets.symmetric(horizontal: Pc.s6, vertical: isActive ? 5 : 2),
        decoration: BoxDecoration(
          color: isActive
              ? Pc.gold.withValues(alpha: 0.16)
              : null,
          borderRadius: BorderRadius.circular(4),
          border: Border(
            left: BorderSide(
              color: isActive ? Pc.gold : Colors.transparent,
              width: 3,
            ),
          ),
        ),
        child: Opacity(
          opacity: p?.bankrupt == true ? 0.4 : 1,
          child: Row(children: [
            SizedBox(
              width: Pc.s16,
              child: isActive
                  ? const Icon(Icons.play_arrow,
                      size: 16, color: Pc.goldDark)
                  : null,
            ),
            // Pawn circle doubles as the live VP leaderboard - and as the
            // anchor every chit addressed to this player flies to. Money that
            // lands somewhere is money you can see arriving.
            Container(
              key: s.stage.anchors.seatKey(i),
              width: 18,
              height: 18,
              alignment: Alignment.center,
              decoration:
                  BoxDecoration(color: pawnColor(i), shape: BoxShape.circle),
              child: rank == null
                  ? null
                  : rank == 1
                      ? const Icon(Icons.workspace_premium,
                          size: 12, color: Pc.text)
                      : Text('$rank',
                          style: const TextStyle(
                              fontSize: 10,
                              fontWeight: FontWeight.bold,
                              color: Pc.text)),
            ),
            const SizedBox(width: Pc.s8),
            Expanded(
              child: Text('$name $tags',
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontWeight: isActive ? FontWeight.bold : null,
                    decoration:
                        p?.bankrupt == true ? TextDecoration.lineThrough : null,
                  )),
            ),
            // A sealed bid, face-up (ADR-0018). Every seat's bid flips at once
            // and is held long enough to compare - this is the single most
            // information-dense moment in Parcello, and the old client never
            // rendered it at all: the auction just silently resolved.
            if (s.stage.bidReveal case final r?)
              if (i < r.bids.length) BidChip(bid: r.bids[i], won: r.winner == i),
            if (p != null)
              Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
                Text('\$${p.cash}',
                    style: const TextStyle(
                        fontFeatures: [FontFeature.tabularFigures()])),
                // Net worth decides a timed game (ADR-0010), so surface it then.
                if (s.gameEndsAt != null)
                  Text(t.sideNetWorth(s.netWorth(i)),
                      style: PcText.caption),
                // Victory-point race (ADR-0020): "the race IS the game".
                if ((s.content?.winVictoryPoints ?? 0) > 0)
                  Text(t.sideVictoryPoints(p.victoryPoints, s.content!.winVictoryPoints),
                      style: const TextStyle(
                          fontSize: 11,
                          color: Pc.goldDark,
                          fontWeight: FontWeight.w700,
                          fontFeatures: [FontFeature.tabularFigures()])),
              ]),
          ]),
        ),
      ));
    }
    // The VP scoring breakdown lives in the center panel now
    // (`_CenterPanel._vpLegend`), where it reads at the table's focus.
    return Column(children: rows);
  }

  Widget _trades(BuildContext context) {
    final v = s.view;
    final t = AppLocalizations.of(context);
    final offers = v?.pendingTrades ?? [];
    String side(int cash, List<int> tiles) {
      final parts = [
        if (cash > 0) '\$$cash',
        ...tiles.map(s.tileName),
      ];
      return parts.isEmpty ? t.tradeNothing : parts.join(' + ');
    }

    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(t.tradesHeader,
          style: const TextStyle(
              fontSize: 12, color: Pc.textMuted, letterSpacing: 1)),
      const SizedBox(height: Pc.s6),
      if (offers.isEmpty)
        Text(t.tradeNoOffers, style: const TextStyle(color: Pc.textMuted)),
      for (final o in offers)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: Pc.s4),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(t.tradeOffer(
                o.id,
                s.playerName(o.from),
                side(o.giveCash, o.giveTiles),
                side(o.receiveCash, o.receiveTiles),
                s.playerName(o.to))),
            Row(children: [
              if (o.to == s.seat) ...[
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'accept_trade', 'trade': o.id}),
                    child: Text(t.actionAccept))),
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'decline_trade', 'trade': o.id}),
                    child: Text(t.tradeRefuse))),
              ],
              if (o.from == s.seat)
                hoverSfx(TextButton(
                    onPressed: () =>
                        s.sendCmd({'type': 'cancel_trade', 'trade': o.id}),
                    child: Text(t.cancel))),
            ]),
          ]),
        ),
      if (v != null && !v.finished)
        hoverSfx(OutlinedButton(
          onPressed: () => showDialog<void>(
              context: context, builder: (ctx) => TradeDialog(s: s)),
          child: Text(t.tradeNewOffer),
        )),
    ]);
  }
}

/// One seat's sealed bid, revealed (ADR-0018). Flips up on the seat marker, in
/// the same instant as everyone else's, and holds - the hold is what makes a
/// simultaneous decision comparable, which is the whole point of showing it.
