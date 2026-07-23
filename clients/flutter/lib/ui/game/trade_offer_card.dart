/// TradeOfferCard (CAR-0002): one pending trade offer as a structured card -
/// who proposes to whom, what is given, what is requested, and the actions
/// available - replacing the old sentence (`t.tradeOffer`, "#{id} {from}
/// gives {give} for {receive} (to {to})").
///
/// PRESENTATIONAL ONLY (DDR-0020): every string arrives already localized and
/// formatted; seat indices and group keys arrive raw so the pawn/band colours
/// resolve from tokens INTERNALLY (never a `Color` in the input), mirroring
/// `SeatTile`'s `pawnColor(seat)` pattern. Never imports `session.dart` or
/// `l10n/`. Action availability (accept/decline/cancel) is the caller's
/// permission logic, unchanged from before this component existed - `null` =
/// hidden, exactly like the labels: a label is only ever read when its
/// callback is non-null.
library;

import 'package:flutter/material.dart';

import '../../design/components/pc_button.dart';
import '../../design/components/pc_card.dart';
import '../../tokens.dart';
import '../../typography.dart';

class TradeOfferCard extends StatelessWidget {
  final int fromSeat;
  final String fromName;
  final int toSeat;
  final String toName;

  /// Formatted cash given/received (e.g. `'$500'`); '' = no cash on that side.
  final String giveCash;
  final String receiveCash;

  /// Already-composed tile display names (mortgage suffix included by the
  /// caller, as `TradeDialog` already does) paired with a group key for the
  /// colour band; `group` null renders the muted/no-group band.
  final List<({String name, String? group})> giveTiles;
  final List<({String name, String? group})> receiveTiles;

  /// Localized fallback for a side that is entirely empty (cash and tiles).
  final String nothingLabel;
  final String givesLabel;
  final String receivesLabel;

  /// Independently nullable: absent = the action is hidden, matching the
  /// caller's existing permission logic (recipient gets accept+decline,
  /// proposer gets cancel, never both for the same offer).
  final VoidCallback? onAccept;
  final VoidCallback? onDecline;
  final VoidCallback? onCancel;

  /// Button labels - only ever read when the matching callback is non-null.
  final String acceptLabel;
  final String declineLabel;
  final String cancelLabel;

  const TradeOfferCard({
    super.key,
    required this.fromSeat,
    required this.fromName,
    required this.toSeat,
    required this.toName,
    required this.giveCash,
    required this.giveTiles,
    required this.receiveCash,
    required this.receiveTiles,
    required this.nothingLabel,
    required this.givesLabel,
    required this.receivesLabel,
    required this.acceptLabel,
    required this.declineLabel,
    required this.cancelLabel,
    this.onAccept,
    this.onDecline,
    this.onCancel,
  });

  @override
  Widget build(BuildContext context) {
    return PcCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _identityRow(),
          const SizedBox(height: Pc.s8),
          _side(givesLabel, giveCash, giveTiles),
          const SizedBox(height: Pc.s6),
          _side(receivesLabel, receiveCash, receiveTiles),
          if (onAccept != null || onDecline != null || onCancel != null) ...[
            const SizedBox(height: Pc.s6),
            _actions(),
          ],
        ],
      ),
    );
  }

  /// Proposer pawn+name (left) -> recipient pawn+name (right). Left-to-right
  /// order IS the direction cue (no arrow/swap icon exists in the project's
  /// icon vocabulary; none is introduced here).
  Widget _identityRow() {
    return Row(
      children: [
        _pawn(fromSeat),
        const SizedBox(width: Pc.s6),
        Expanded(
          child: Text(
            fromName,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: PcText.rowTitle,
          ),
        ),
        const SizedBox(width: Pc.s8),
        Expanded(
          child: Text(
            toName,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.end,
            style: PcText.rowTitle,
          ),
        ),
        const SizedBox(width: Pc.s6),
        _pawn(toSeat),
      ],
    );
  }

  Widget _pawn(int seat) => Container(
    width: 14,
    height: 14,
    decoration: BoxDecoration(color: pawnColor(seat), shape: BoxShape.circle),
  );

  /// One side (DONNE or REÇOIT). Collapses to a single line when there is
  /// cash AND exactly one tile (the common case); otherwise every item
  /// (cash, each tile) gets its own line so an uncapped tile count never
  /// looks cramped. Empty (no cash, no tiles) shows `nothingLabel`.
  Widget _side(
    String label,
    String cash,
    List<({String name, String? group})> tiles,
  ) {
    final hasCash = cash.isNotEmpty;
    final rows = <Widget>[];
    if (hasCash && tiles.length == 1) {
      rows.add(
        Row(
          children: [
            Text(cash, style: PcText.amount),
            const SizedBox(width: Pc.s6),
            Expanded(child: _tileRow(tiles.single)),
          ],
        ),
      );
    } else {
      if (hasCash) rows.add(Text(cash, style: PcText.amount));
      for (final tile in tiles) {
        rows.add(
          Padding(
            padding: const EdgeInsets.only(top: Pc.s2),
            child: _tileRow(tile),
          ),
        );
      }
      if (!hasCash && tiles.isEmpty) {
        rows.add(Text(nothingLabel, style: PcText.caption));
      }
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: PcText.whisper.copyWith(color: Pc.textMuted, letterSpacing: 1),
        ),
        const SizedBox(height: Pc.s2),
        ...rows,
      ],
    );
  }

  /// A group-colour band beside the tile name - the same visual idea as
  /// PropertyPanel's header band, built fresh here (PropertyPanel exposes no
  /// reusable sub-widget; see CAR-0002 S4).
  Widget _tileRow(({String name, String? group}) tile) {
    return Row(
      children: [
        Container(
          width: Pc.s4,
          height: 14,
          color: groupColors[tile.group] ?? Pc.textFaint,
        ),
        const SizedBox(width: Pc.s6),
        Expanded(
          child: Text(
            tile.name,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: PcText.body,
          ),
        ),
      ],
    );
  }

  /// Same styling as the sentence-based row it replaces: `quiet`,
  /// intrinsic-width buttons, right-aligned.
  Widget _actions() {
    return Row(
      mainAxisAlignment: MainAxisAlignment.end,
      children: [
        if (onDecline != null)
          PcButton(
            declineLabel,
            onPressed: onDecline,
            variant: PcButtonVariant.quiet,
            wide: false,
          ),
        if (onAccept != null) ...[
          const SizedBox(width: Pc.s6),
          PcButton(
            acceptLabel,
            onPressed: onAccept,
            variant: PcButtonVariant.quiet,
            wide: false,
          ),
        ],
        if (onCancel != null)
          PcButton(
            cancelLabel,
            onPressed: onCancel,
            variant: PcButtonVariant.quiet,
            wide: false,
          ),
      ],
    );
  }
}
