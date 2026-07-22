/// The property card (game-screen refonte, DDR-0021 right region): the tile the
/// player is on - or is hovering/focusing - shown as a title-deed: group band,
/// name, owner, the real rent ladder (`TileDef.rents`, already on the wire), the
/// development level, and the list/market price.
///
/// Honest by construction: it renders the tile's DEFINED rent schedule (the
/// authoritative content), never a fabricated number. The amount actually
/// collected at a landing is further moved by group/boost/market/spotlight and
/// is shown live in the event feed - not claimed here.
library;

import 'package:flutter/material.dart';

import '../../design/components/pc_card.dart';
import '../../l10n/app_localizations.dart';
import '../../protocol.dart';
import '../../session.dart';
import '../../tokens.dart';
import '../../typography.dart';

class PropertyPanel extends StatelessWidget {
  final GameSession s;

  /// Board index of the tile to show (the caller picks hovered-else-standing).
  final int tile;

  const PropertyPanel({super.key, required this.s, required this.tile});

  @override
  Widget build(BuildContext context) {
    final t = AppLocalizations.of(context);
    final content = s.content;
    final v = s.view;
    if (content == null || tile < 0 || tile >= content.board.length) {
      return const SizedBox.shrink();
    }
    final def = content.board[tile];
    final ts = v?.tiles.elementAtOrNull(tile);
    final band = def.isProperty
        ? (groupColors[def.group] ?? Pc.textFaint)
        : Pc.borderMuted;

    final header = Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Container(width: Pc.s4, height: 34, color: band),
      const SizedBox(width: Pc.s8),
      Expanded(
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(def.name, style: PcText.tileTitle, maxLines: 1,
              overflow: TextOverflow.ellipsis),
          Text(def.isProperty ? (def.group ?? '') : t.propNotProperty,
              style: PcText.caption.copyWith(color: Pc.textMuted)),
        ]),
      ),
      if (ts?.mortgaged == true)
        Text(t.propMortgaged, style: PcText.whisper.copyWith(color: Pc.oxblood)),
    ]);

    final children = <Widget>[header];

    if (def.isProperty) {
      // Owner line.
      final ownerSeat = ts?.owner;
      children.add(const SizedBox(height: Pc.s6));
      children.add(Row(children: [
        Text('${t.propOwner} ',
            style: PcText.caption.copyWith(color: Pc.textMuted)),
        Expanded(
          child: Text(
            ownerSeat == null ? t.propUnowned : s.playerName(ownerSeat),
            style: PcText.label.copyWith(
                color: ownerSeat == null ? Pc.textFaint : Pc.text),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ]));

      // The rent ladder (real schedule). Highlight the current level.
      if (def.rents.isNotEmpty) {
        final houses = ts?.houses ?? 0;
        final currentLevel =
            def.rentModel == 'houses' ? houses : _ownedInGroup(def, v) - 1;
        children.add(const SizedBox(height: Pc.s6));
        children.add(Text(t.propRentLadder,
            style: PcText.whisper
                .copyWith(color: Pc.textMuted, letterSpacing: 1)));
        for (var lvl = 0; lvl < def.rents.length; lvl++) {
          if (def.rents[lvl] <= 0 && lvl > 0) continue; // skip empty scaled rows
          children.add(_rentRow(t, def, lvl, def.rents[lvl], lvl == currentLevel));
        }
      }

      // Development + price.
      children.add(const SizedBox(height: Pc.s6));
      final maxHouses = (def.rents.length - 1).clamp(1, 5);
      final houses = ts?.houses ?? 0;
      final devLabel = def.rentModel == 'houses'
          ? (houses >= maxHouses && maxHouses > 0
              ? t.propDevHotel
              : t.propDevHouses(houses, maxHouses))
          : '';
      final price = def.price ?? 0;
      final now = marketPrice(def, v);
      children.add(Row(children: [
        if (devLabel.isNotEmpty)
          Text(devLabel, style: PcText.caption.copyWith(color: Pc.textMuted)),
        const Spacer(),
        Text('${t.propPrice} ${_money(price)}',
            style: PcText.caption.copyWith(color: Pc.textMuted)),
        if (now != price) ...[
          const SizedBox(width: Pc.s6),
          Text(t.propMarketNow(_money(now)),
              style: PcText.caption.copyWith(color: Pc.gold)),
        ],
      ]));
    }

    return PcCard(
      padding: const EdgeInsets.all(10),
      child:
          Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: children),
    );
  }

  /// One rung of the rent ladder: level marker (houses / hotel) + rent value.
  Widget _rentRow(
      AppLocalizations t, TileDef def, int lvl, int rent, bool current) {
    final maxHouses = (def.rents.length - 1).clamp(1, 5);
    final isHotel = def.rentModel == 'houses' && lvl >= maxHouses && lvl > 0;
    final marker = def.rentModel == 'houses'
        ? (isHotel
            ? const Icon(Icons.apartment, size: 14, color: Pc.gold)
            : lvl == 0
                ? Text(t.propRentUnimproved,
                    style: PcText.whisper.copyWith(color: Pc.textMuted))
                : Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      for (var h = 0; h < lvl; h++)
                        const Icon(Icons.home, size: 11, color: Pc.sage),
                    ],
                  ))
        : Text('${lvl + 1}',
            style: PcText.whisper.copyWith(color: Pc.textMuted));

    return Container(
      margin: const EdgeInsets.symmetric(vertical: 1),
      padding: const EdgeInsets.symmetric(horizontal: Pc.s6, vertical: Pc.s2),
      decoration: current
          ? BoxDecoration(color: Pc.goldWash, borderRadius: Pc.radius)
          : null,
      child: Row(children: [
        SizedBox(width: 64, child: marker),
        const Spacer(),
        Text(_money(rent),
            style: (current ? PcText.rowTitle : PcText.body).copyWith(
                color: current ? Pc.gold : Pc.text)),
      ]),
    );
  }

  /// Group tiles the owner of [def]'s tile owns (for the group_scaled ladder).
  int _ownedInGroup(TileDef def, ClientView? v) {
    if (v == null) return 0;
    final owner = v.tiles.elementAtOrNull(tile)?.owner;
    if (owner == null) return 0;
    var n = 0;
    final board = s.content!.board;
    for (var i = 0; i < board.length && i < v.tiles.length; i++) {
      if (board[i].group == def.group && v.tiles[i].owner == owner) n++;
    }
    return n;
  }

  String _money(int n) => '\$$n';
}
