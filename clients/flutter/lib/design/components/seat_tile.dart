/// SeatTile - one player's seat row: the game's most-repeated, most
/// Parcello-specific card (DESIGN_SYSTEM.md #13). Identity (pawn colour + name),
/// live cash / victory-points / net-worth, and turn / bankruptcy / rank state.
/// It is also the TARGET every money chit flies to (the `anchorKey` sits on the
/// pawn circle) and where a sealed bid flips face-up (`trailingBid`).
///
/// Part of the in-tree design system (DDR-0016). PRESENTATIONAL by design: it
/// takes already-resolved, already-localized strings (INVARIANTS C1 keeps l10n
/// in the app layer, not here) and the stage-owned `anchorKey`/`trailingBid` -
/// never `GameSession`. The parent computes the cross-seat bits (VP rank, the
/// round metronome) and formats the labels; the tile only draws.
///
/// PUBLIC API - STABILITY CONTRACT (DDR-0019): the constructor + named params
/// are public API; grow additively as real screens demand, never speculatively.
library;

import 'package:flutter/material.dart';

import '../../motion.dart';
import '../../tokens.dart';
import '../../typography.dart';

class SeatTile extends StatelessWidget {
  /// Seat index - drives the pawn colour (`pawnColor`) and identity.
  final int seat;

  /// The player's display name (resolved: player, else seat, else fallback).
  final String name;

  /// Pre-joined, localized status tags (you / bot / offline / jail / route /
  /// hand-cycled); empty string for none. Domain logic (which tags apply)
  /// stays in the parent - the tile only renders the result.
  final String tags;

  /// Formatted cash (e.g. `$1200`); null before the game starts (lobby seat),
  /// which also hides the whole trailing figures column.
  final String? cash;

  /// Formatted victory-points label; null when the VP race is off.
  final String? vpLabel;

  /// Formatted net-worth label; null unless the game is time-boxed (net worth
  /// decides a timed game, so it is surfaced only then).
  final String? netWorthLabel;

  /// VP leaderboard rank (1 = leading, a crown); null = bankrupt / race off.
  final int? rank;

  /// Whose turn it is: a highlighted row, a gold left border, a leading marker.
  final bool active;

  /// Eliminated: dimmed and struck through (but still shown - the table sees
  /// who fell).
  final bool bankrupt;

  /// The chit anchor (stage) - placed on the pawn circle so money that lands
  /// on this seat visibly arrives there. Omit outside a live game.
  final Key? anchorKey;

  /// The sealed-bid reveal chip (stage), shown between the name and the
  /// figures at auction settlement. Omit when no bid is revealed.
  final Widget? trailingBid;

  const SeatTile({
    super.key,
    required this.seat,
    required this.name,
    required this.tags,
    required this.active,
    required this.bankrupt,
    this.cash,
    this.vpLabel,
    this.netWorthLabel,
    this.rank,
    this.anchorKey,
    this.trailingBid,
  });

  @override
  Widget build(BuildContext context) {
    return AnimatedContainer(
      duration: Motion.stateFade,
      margin: const EdgeInsets.symmetric(vertical: Pc.s2),
      padding: EdgeInsets.symmetric(horizontal: Pc.s6, vertical: active ? 5 : 2),
      decoration: BoxDecoration(
        color: active ? Pc.gold.withValues(alpha: 0.16) : null,
        borderRadius: Pc.radius,
        border: Border(
          left: BorderSide(
            color: active ? Pc.gold : Colors.transparent,
            width: 3,
          ),
        ),
      ),
      child: Opacity(
        opacity: bankrupt ? 0.4 : 1,
        child: Row(children: [
          SizedBox(
            width: Pc.s16,
            child: active
                ? const Icon(Icons.play_arrow, size: 16, color: Pc.goldDark)
                : null,
          ),
          // Pawn circle doubles as the live VP leaderboard AND the chit anchor.
          Container(
            key: anchorKey,
            width: 18,
            height: 18,
            alignment: Alignment.center,
            decoration:
                BoxDecoration(color: pawnColor(seat), shape: BoxShape.circle),
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
            child: Text(tags.isEmpty ? name : '$name $tags',
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontWeight: active ? FontWeight.bold : null,
                  decoration:
                      bankrupt ? TextDecoration.lineThrough : null,
                )),
          ),
          ?trailingBid,
          if (cash != null)
            Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
              Text(cash!, style: PcText.amount),
              if (netWorthLabel != null)
                Text(netWorthLabel!, style: PcText.caption),
              if (vpLabel != null)
                Text(vpLabel!,
                    style: PcText.amount.copyWith(
                        fontSize: 11,
                        fontWeight: FontWeight.w700,
                        color: Pc.goldDark)),
            ]),
        ]),
      ),
    );
  }
}
