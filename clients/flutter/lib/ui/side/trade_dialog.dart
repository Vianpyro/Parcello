/// The trade composer (ADR-0007): offers are private to their two parties.
library;

import 'package:flutter/material.dart';

import '../../l10n/app_localizations.dart';
import '../../session.dart';
import '../../sfx.dart';
import '../../tokens.dart';
import '../../typography.dart';

class TradeDialog extends StatefulWidget {
  final GameSession s;
  const TradeDialog({super.key, required this.s});

  @override
  State<TradeDialog> createState() => _TradeDialogState();
}

class _TradeDialogState extends State<TradeDialog> {
  int? _to;
  final _giveCash = TextEditingController(text: '0');
  final _receiveCash = TextEditingController(text: '0');
  final _giveTiles = <String>{};
  final _receiveTiles = <String>{};

  @override
  Widget build(BuildContext context) {
    final s = widget.s;
    final t = AppLocalizations.of(context);
    final v = s.view!;
    final candidates = [
      for (var i = 0; i < v.players.length; i++)
        if (i != s.seat && !v.players[i].bankrupt) i,
    ];
    _to ??= candidates.firstOrNull;

    Widget tileList(int? seat, Set<String> picked) {
      final tiles = [
        for (var i = 0; i < s.content!.board.length; i++)
          if (seat != null &&
              v.tiles[i].owner == seat &&
              s.content!.board[i].isProperty)
            i,
      ];
      return SizedBox(
        height: 140,
        width: 200,
        child: ListView(children: [
          for (final i in tiles)
            CheckboxListTile(
              dense: true,
              value: picked.contains(s.content!.board[i].id),
              title: Text(
                s.tileName(i) + (v.tiles[i].mortgaged ? ' (M)' : ''),
                style: PcText.label,
              ),
              onChanged: (on) => setState(() {
                final id = s.content!.board[i].id;
                on == true ? picked.add(id) : picked.remove(id);
              }),
            ),
        ]),
      );
    }

    Widget cashField(TextEditingController c) => SizedBox(
          width: 200,
          child: TextField(
            controller: c,
            keyboardType: TextInputType.number,
            decoration:
                InputDecoration(labelText: t.cashLabel, isDense: true),
          ),
        );

    return AlertDialog(
      title: Text(t.tradeNewOfferTitle),
      content: Column(mainAxisSize: MainAxisSize.min, children: [
        DropdownButton<int>(
          value: _to,
          isExpanded: true,
          items: [
            for (final i in candidates)
              DropdownMenuItem(value: i, child: Text(s.playerName(i))),
          ],
          onChanged: (i) => setState(() {
            _to = i;
            _receiveTiles.clear();
          }),
        ),
        Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Column(children: [
            Text(t.tradeYouGive),
            cashField(_giveCash),
            tileList(s.seat, _giveTiles),
          ]),
          const SizedBox(width: Pc.s12),
          Column(children: [
            Text(t.tradeYouWant),
            cashField(_receiveCash),
            tileList(_to, _receiveTiles),
          ]),
        ]),
      ]),
      actions: [
        hoverSfx(TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(t.close))),
        hoverSfx(FilledButton(
          onPressed: _to == null
              ? null
              : () {
                  widget.s.sendCmd({
                    'type': 'propose_trade',
                    'to': v.players[_to!].id,
                    'give_cash': int.tryParse(_giveCash.text) ?? 0,
                    'give_tiles': _giveTiles.toList(),
                    'receive_cash': int.tryParse(_receiveCash.text) ?? 0,
                    'receive_tiles': _receiveTiles.toList(),
                  });
                  Navigator.pop(context);
                },
          child: Text(t.tradePropose),
        )),
      ],
    );
  }
}
