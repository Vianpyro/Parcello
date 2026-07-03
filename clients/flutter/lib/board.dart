/// Board rendering: classic 40-tile counter-clockwise ring on an 11x11 grid
/// (same cell walk as the reference web client), wrap fallback for modded
/// board sizes. Pure projection of content + view; taps bubble up.
library;

import 'package:flutter/material.dart';

import 'protocol.dart';

const pawnColors = [
  Color(0xFFC0564F),
  Color(0xFF3D7DC0),
  Color(0xFF4D9E5A),
  Color(0xFFC8963F),
  Color(0xFF8A5FB3),
  Color(0xFF4FA3A8),
];

const _groupColors = <String, Color>{
  'brown': Color(0xFF8A5A3B),
  'lightblue': Color(0xFF8FC7E8),
  'pink': Color(0xFFD76FA3),
  'orange': Color(0xFFE08A3C),
  'red': Color(0xFFC0564F),
  'yellow': Color(0xFFE0C93C),
  'green': Color(0xFF4D9E5A),
  'navy': Color(0xFF3B5A8A),
  'transit': Color(0xFF444444),
  'works': Color(0xFF999999),
};

/// Grid cell (1-based row/col) of tile `i` on the classic 40-tile ring:
/// 0 is the bottom-right corner, walking counter-clockwise.
({int r, int c}) cellOf(int i) {
  if (i <= 10) return (r: 11, c: 11 - i);
  if (i <= 20) return (r: 11 - (i - 10), c: 1);
  if (i <= 30) return (r: 1, c: i - 19);
  return (r: i - 29, c: 11);
}

class BoardWidget extends StatelessWidget {
  final GameContent content;
  final ClientView? view;
  final int? mySeat;
  final void Function(int tile) onTileTap;
  final Widget center;

  const BoardWidget({
    super.key,
    required this.content,
    required this.view,
    required this.mySeat,
    required this.onTileTap,
    required this.center,
  });

  @override
  Widget build(BuildContext context) {
    final n = content.board.length;
    if (n != 40) return _wrapLayout();
    return AspectRatio(
      aspectRatio: 1,
      child: LayoutBuilder(builder: (context, box) {
        final w = box.maxWidth / 11, h = box.maxHeight / 11;
        return Stack(children: [
          Positioned(
            left: w,
            top: h,
            width: w * 9,
            height: h * 9,
            child: Container(
              margin: const EdgeInsets.all(2),
              padding: const EdgeInsets.all(8),
              color: const Color(0xFFDFE7D8),
              child: center,
            ),
          ),
          for (var i = 0; i < 40; i++)
            Positioned(
              left: (cellOf(i).c - 1) * w,
              top: (cellOf(i).r - 1) * h,
              width: w,
              height: h,
              child: _tile(i),
            ),
        ]);
      }),
    );
  }

  // Non-40 boards (mods): plain wrap, the center panel goes below.
  Widget _wrapLayout() {
    return Column(children: [
      Wrap(
        spacing: 2,
        runSpacing: 2,
        children: [
          for (var i = 0; i < content.board.length; i++)
            SizedBox(width: 90, height: 80, child: _tile(i)),
        ],
      ),
      const SizedBox(height: 8),
      Expanded(
        child: Container(
          padding: const EdgeInsets.all(8),
          color: const Color(0xFFDFE7D8),
          child: center,
        ),
      ),
    ]);
  }

  Widget _tile(int i) {
    final def = content.board[i];
    final ts = view?.tiles.elementAtOrNull(i);
    final pawns = <int>[
      if (view != null)
        for (var s = 0; s < view!.players.length; s++)
          if (!view!.players[s].bankrupt && view!.players[s].position == i) s,
    ];
    final band = def.isProperty
        ? (_groupColors[def.group] ?? const Color(0xFF777777))
        : const Color(0xFFB9C2B0);
    return GestureDetector(
      onTap: () => onTileTap(i),
      child: Container(
        margin: const EdgeInsets.all(1),
        decoration: BoxDecoration(
          color: const Color(0xFFF4F7EF),
          borderRadius: BorderRadius.circular(2),
          border: ts?.owner != null && ts!.owner == mySeat
              ? Border.all(color: const Color(0xFF2F6F3E), width: 2)
              : null,
        ),
        child: Stack(children: [
          Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Container(height: 7, color: band),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(2, 1, 2, 0),
                child: Text(
                  def.name,
                  style: const TextStyle(fontSize: 8, color: Color(0xFF222222)),
                  overflow: TextOverflow.fade,
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(2, 0, 2, 1),
              child: Text(
                _meta(def, ts),
                style: TextStyle(
                  fontSize: 7,
                  color: ts?.mortgaged == true
                      ? const Color(0xFFC0564F)
                      : const Color(0xFF555555),
                ),
              ),
            ),
          ]),
          if (ts?.owner != null)
            Positioned(
              top: 1,
              right: 1,
              child: Container(
                width: 7,
                height: 7,
                color: pawnColors[ts!.owner! % pawnColors.length],
              ),
            ),
          Positioned(
            bottom: 2,
            left: 2,
            child: Row(children: [
              for (final s in pawns)
                Container(
                  width: 9,
                  height: 9,
                  margin: const EdgeInsets.only(right: 2),
                  decoration: BoxDecoration(
                    color: pawnColors[s % pawnColors.length],
                    shape: BoxShape.circle,
                    border: Border.all(color: Colors.black54),
                  ),
                ),
            ]),
          ),
        ]),
      ),
    );
  }

  String _meta(TileDef def, TileState? ts) {
    final parts = <String>[];
    if (def.isProperty) parts.add('\$${def.price}');
    if (def.amount != null) parts.add('pay \$${def.amount}');
    if (ts != null && ts.houses > 0) {
      parts.add(ts.houses == 5 ? 'HOTEL' : '▪' * ts.houses);
    }
    if (ts?.mortgaged == true) parts.add('MORT.');
    return parts.join(' ');
  }
}
