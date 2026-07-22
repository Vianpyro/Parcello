/// The right-hand column: seats, room, trades, and the end-of-game cards.
/// It scrolls because it grows with the room (six offers overflow a Deck).
library;

import 'package:flutter/material.dart';

import '../../design/components/pc_button.dart';
import '../../design/components/pc_card.dart';
import '../../design/components/pc_dialog.dart';
import '../../design/components/seat_tile.dart';
import '../../l10n/app_localizations.dart';
import '../../protocol.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';
import '../coach_mark.dart';
import '../common.dart';
import '../game/property_panel.dart';
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
    // The property card shows the tile under the cursor/focus, else the one the
    // player is standing on (DDR-0021 right region; first slice, still hosted in
    // the side panel until the full reflow moves it to its own region).
    final focusTile = (v != null && !v.finished)
        ? (s.hoverTile ??
            (s.seat != null ? v.players.elementAtOrNull(s.seat!)?.position : null))
        : null;
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
        PcCard(
          raised: true,
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
      // Game over: replay together, or go back to the start screen.
      if (v != null && v.finished)
        PcCard(
          raised: true,
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
                    Expanded(
                        child: PcButton(t.playAgain,
                            onPressed: s.sendPlayAgain)),
                    const SizedBox(width: Pc.s8),
                  ],
                  Expanded(
                      child: PcButton(t.continueLabel,
                          onPressed: s.leaveRoom,
                          variant: PcButtonVariant.secondary)),
                ]),
                if (!s.spectating)
                  Text(t.playAgainHint, style: PcText.caption),
              ]),
        ),
      if (focusTile != null && s.content != null) ...[
        const SizedBox(height: Pc.s6),
        PropertyPanel(s: s, tile: focusTile),
        const SizedBox(height: Pc.s6),
      ],
      PcCard(
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
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
              PcButton(t.startGame,
                  onPressed:
                      s.seat == 0 && s.seats.length >= 2 ? s.sendStart : null),
              // Host-only bot controls. Bots fill empty seats but yield to
              // humans, so they never block a join (ADR-0014).
              if (s.seat == 0)
                Padding(
                  padding: const EdgeInsets.only(top: Pc.s6),
                  child: Row(children: [
                    Expanded(
                        child: PcButton(t.addBot,
                            onPressed: s.seats.length < 6 ? s.addBot : null,
                            variant: PcButtonVariant.secondary)),
                    const SizedBox(width: Pc.s6),
                    Expanded(
                        child: PcButton(t.removeBot,
                            onPressed: s.seats.any((x) => x.isBot)
                                ? s.removeBot
                                : null,
                            variant: PcButtonVariant.secondary)),
                  ]),
                ),
              if (s.code != null)
                Padding(
                  padding: const EdgeInsets.only(top: Pc.s6),
                  child: PcButton(t.copyCodeToShare,
                      onPressed: () => copyCode(context, s.code!),
                      variant: PcButtonVariant.secondary),
                ),
              if (s.settings != null) SettingsPanel(s: s),
              // Cancel: leave the room (dissolves it for the host) and return
              // to the main menu. Keyboard/controller reachable like any button.
              const SizedBox(height: Pc.s6),
              PcButton(t.backToMenu,
                  onPressed: s.leaveRoom, variant: PcButtonVariant.secondary),
            ],
          ]),
        ),
      PcCard(child: _trades(context)),
      // Post-game survey: an ordinary side card, never a modal - it must
      // not block anything (no frustration by design).
      // The survey asks players about a game they played; a spectator's
      // submission would only bounce off the server (ADR-0035).
      if (s.view?.finished == true && !s.feedbackDone && !s.spectating)
        FeedbackCard(s: s),
      // The resign TRIGGER stays a bespoke restrained outlined-oxblood button:
      // PcButton has no "outlined destructive" variant (destructive is filled
      // red, too loud for an always-visible control) - a documented gap
      // (DESIGN_FEEDBACK #3). The CONFIRM step, however, is a PcDialog.
      PcCard(
        child: hoverSfx(OutlinedButton(
          style: OutlinedButton.styleFrom(foregroundColor: Pc.oxblood),
          onPressed: () async {
            final ok = await showDialog<bool>(
              context: context,
              builder: (ctx) => PcDialog(
                title: t.resignConfirmTitle,
                cancelLabel: t.cancel,
                primaryLabel: t.resign,
                destructive: true,
                onPrimary: () {
                  sfx.buttonYes();
                  Navigator.pop(ctx, true);
                },
              ),
            );
            if (ok == true) s.sendCmd({'type': 'resign'});
          },
          child: Text(t.resign),
        )),
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
    // A sealed bid, face-up (ADR-0018): every seat's bid flips at once and is
    // held long enough to compare - the single most information-dense moment in
    // Parcello, which the old client never rendered (the auction just resolved).
    final reveal = s.stage.bidReveal;
    final showVp = (s.content?.winVictoryPoints ?? 0) > 0;
    for (var i = 0; i < count; i++) {
      final p = v?.players.elementAtOrNull(i);
      final seatInfo = s.seats.elementAtOrNull(i);
      final name = p?.name ?? seatInfo?.name ?? t.seatFallback(i);
      // Whose turn is it: bold text alone read as too subtle in playtests
      // (2026-07) - a highlighted row + a leading marker reads at a glance.
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
      rows.add(SeatTile(
        seat: i,
        name: name,
        tags: tags,
        active: isActive,
        bankrupt: p?.bankrupt == true,
        rank: ranks[i],
        anchorKey: s.stage.anchors.seatKey(i),
        cash: p != null ? '\$${p.cash}' : null,
        // Net worth decides a timed game (ADR-0010), so surface it only then.
        netWorthLabel: p != null && s.gameEndsAt != null
            ? t.sideNetWorth(s.netWorth(i))
            : null,
        // Victory-point race (ADR-0020): "the race IS the game".
        vpLabel: p != null && showVp
            ? t.sideVictoryPoints(p.victoryPoints, s.content!.winVictoryPoints)
            : null,
        trailingBid: reveal != null && i < reveal.bids.length
            ? BidChip(bid: reveal.bids[i], won: reveal.winner == i)
            : null,
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
