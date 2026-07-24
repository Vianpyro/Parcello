/// The scrolling remainder of the right region (DDR-0021): room, trades, and
/// the end-of-game cards. It scrolls because it grows with the room (six offers
/// overflow a Deck). The property deed is no longer here - it moved to its own
/// stable slot above this stack (`_RightColumn` in game_screen), which is also
/// where the hover-else-standing tile is now chosen.
/// Resign is NOT here - it moved to NavRail's Menu (game-screen refonte),
/// gated on `GameSession.canResign`.
library;

import 'package:flutter/material.dart';

import '../../design/components/pc_button.dart';
import '../../design/components/pc_card.dart';
import '../../l10n/app_localizations.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';
import '../coach_mark.dart';
import '../common.dart';
import '../game/trade_offer_card.dart';
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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
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
            child: Row(
              children: [
                const Icon(Icons.visibility_outlined, color: Pc.gold, size: 18),
                const SizedBox(width: Pc.s8),
                Expanded(
                  child: Text(
                    t.spectatingBadge,
                    style: PcText.label.copyWith(color: Pc.textMuted),
                  ),
                ),
                PcButton(
                  t.continueLabel,
                  onPressed: s.leaveRoom,
                  variant: PcButtonVariant.quiet,
                  wide: false,
                ),
              ],
            ),
          ),
        // Game over: replay together, or go back to the start screen.
        if (v != null && v.finished)
          PcCard(
            raised: true,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  t.sideWinnerWins(s.playerName(v.winner!)),
                  style: PcText.section.copyWith(color: Pc.gold),
                ),
                const SizedBox(height: Pc.s8),
                Row(
                  children: [
                    // Spectators cannot replay a game they never sat in
                    // (ADR-0035); they only get the way out.
                    if (!s.spectating) ...[
                      Expanded(
                        child: PcButton(
                          t.playAgain,
                          onPressed: s.sendPlayAgain,
                        ),
                      ),
                      const SizedBox(width: Pc.s8),
                    ],
                    Expanded(
                      child: PcButton(
                        t.continueLabel,
                        onPressed: s.leaveRoom,
                        variant: PcButtonVariant.secondary,
                      ),
                    ),
                  ],
                ),
                if (!s.spectating) Text(t.playAgainHint, style: PcText.caption),
              ],
            ),
          ),
        PcCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      t.sideRoom(s.code ?? ''),
                      style: PcText.rowTitle.copyWith(
                        color: Pc.gold,
                        letterSpacing: 2,
                      ),
                    ),
                  ),
                  if (s.code != null)
                    hoverSfx(
                      IconButton(
                        iconSize: 18,
                        visualDensity: VisualDensity.compact,
                        tooltip: t.copyRoomCode,
                        icon: const Icon(Icons.copy, color: Pc.textMuted),
                        onPressed: () => copyCode(context, s.code!),
                      ),
                    ),
                ],
              ),
              const SizedBox(height: Pc.s6),
              if (s.view == null) ...[
                const SizedBox(height: Pc.s8),
                PcButton(
                  t.startGame,
                  onPressed: s.seat == 0 && s.seats.length >= 2
                      ? s.sendStart
                      : null,
                ),
                // Host-only bot controls. Bots fill empty seats but yield to
                // humans, so they never block a join (ADR-0014).
                if (s.seat == 0)
                  Padding(
                    padding: const EdgeInsets.only(top: Pc.s6),
                    child: Row(
                      children: [
                        Expanded(
                          child: PcButton(
                            t.addBot,
                            onPressed: s.seats.length < 6 ? s.addBot : null,
                            variant: PcButtonVariant.secondary,
                          ),
                        ),
                        const SizedBox(width: Pc.s6),
                        Expanded(
                          child: PcButton(
                            t.removeBot,
                            onPressed: s.seats.any((x) => x.isBot)
                                ? s.removeBot
                                : null,
                            variant: PcButtonVariant.secondary,
                          ),
                        ),
                      ],
                    ),
                  ),
                if (s.code != null)
                  Padding(
                    padding: const EdgeInsets.only(top: Pc.s6),
                    child: PcButton(
                      t.copyCodeToShare,
                      onPressed: () => copyCode(context, s.code!),
                      variant: PcButtonVariant.secondary,
                    ),
                  ),
                if (s.settings != null) SettingsPanel(s: s),
                // Cancel: leave the room (dissolves it for the host) and return
                // to the main menu. Keyboard/controller reachable like any button.
                const SizedBox(height: Pc.s6),
                PcButton(
                  t.backToMenu,
                  onPressed: s.leaveRoom,
                  variant: PcButtonVariant.secondary,
                ),
              ],
            ],
          ),
        ),
        PcCard(child: _trades(context)),
        ..._tradeCards(context),
        // Post-game survey: an ordinary side card, never a modal - it must
        // not block anything (no frustration by design).
        // The survey asks players about a game they played; a spectator's
        // submission would only bounce off the server (ADR-0035).
        if (s.view?.finished == true && !s.feedbackDone && !s.spectating)
          FeedbackCard(s: s),
        // Resign lives only in NavRail's Menu now (gated on `s.canResign`) -
        // this panel no longer duplicates the trigger or the confirm flow.
      ],
    );
  }

  /// The section header card: label, the empty-state message, and the "new
  /// offer" trigger. Individual offers are no longer rows inside this card -
  /// each is its own `TradeOfferCard` (CAR-0002), appended after it in
  /// `build()`, so a structured card never nests inside another flat card.
  Widget _trades(BuildContext context) {
    final v = s.view;
    final t = AppLocalizations.of(context);
    final offers = v?.pendingTrades ?? [];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          t.tradesHeader,
          style: PcText.label.copyWith(color: Pc.textMuted, letterSpacing: 1),
        ),
        const SizedBox(height: Pc.s6),
        if (offers.isEmpty) Text(t.tradeNoOffers, style: PcText.caption),
        if (v != null && !v.finished)
          PcButton(
            t.tradeNewOffer,
            onPressed: () => showDialog<void>(
              context: context,
              builder: (ctx) => TradeDialog(s: s),
            ),
            variant: PcButtonVariant.secondary,
            wide: false,
          ),
      ],
    );
  }

  /// One `TradeOfferCard` per pending offer (CAR-0002), replacing the old
  /// sentence-based row. This method keeps exactly the permission logic the
  /// row used to have: the recipient (`o.to == s.seat`) gets accept+decline,
  /// the proposer (`o.from == s.seat`) gets cancel, never both for the same
  /// offer - the component itself decides nothing, it only hides whichever
  /// callback arrives null.
  List<Widget> _tradeCards(BuildContext context) {
    final v = s.view;
    final t = AppLocalizations.of(context);
    final offers = v?.pendingTrades ?? [];

    String cashLabel(int cash) => cash > 0 ? '\$$cash' : '';
    // Mirrors TradeDialog's tile-name convention (name + a mortgaged
    // suffix); both now share the same localized `t.tileMortgagedSuffix`
    // key instead of a raw literal.
    List<({String name, String? group})> tileEntries(List<int> tiles) => [
      for (final i in tiles)
        (
          name:
              s.tileName(i) +
              (v!.tiles[i].mortgaged ? t.tileMortgagedSuffix : ''),
          group: s.content?.board.elementAtOrNull(i)?.group,
        ),
    ];

    return [
      for (final o in offers)
        Padding(
          padding: const EdgeInsets.only(bottom: Pc.s6),
          child: TradeOfferCard(
            fromSeat: o.from,
            fromName: s.playerName(o.from),
            toSeat: o.to,
            toName: s.playerName(o.to),
            giveCash: cashLabel(o.giveCash),
            giveTiles: tileEntries(o.giveTiles),
            receiveCash: cashLabel(o.receiveCash),
            receiveTiles: tileEntries(o.receiveTiles),
            nothingLabel: t.tradeNothing,
            givesLabel: t.tradeGivesLabel,
            receivesLabel: t.tradeReceivesLabel,
            acceptLabel: t.actionAccept,
            declineLabel: t.tradeRefuse,
            cancelLabel: t.cancel,
            onAccept: o.to == s.seat
                ? () => s.sendCmd({'type': 'accept_trade', 'trade': o.id})
                : null,
            onDecline: o.to == s.seat
                ? () => s.sendCmd({'type': 'decline_trade', 'trade': o.id})
                : null,
            onCancel: o.from == s.seat
                ? () => s.sendCmd({'type': 'cancel_trade', 'trade': o.id})
                : null,
          ),
        ),
    ];
  }
}
