/// Board rendering: classic 40-tile counter-clockwise ring on an 11x11 grid
/// (same cell walk as the reference web client), wrap fallback for modded
/// board sizes. Pure projection of content + view; taps bubble up. Pawns
/// live in an animated overlay (`_PawnLayer`) that glides them tile by tile
/// so a move is actually visible.
library;

import 'dart:math' as math;

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

  List<PawnData> _pawns() {
    final v = view;
    if (v == null) return const [];
    return [
      for (var s = 0; s < v.players.length; s++)
        if (!v.players[s].bankrupt)
          PawnData(
            seat: s,
            color: pawnColors[s % pawnColors.length],
            position: v.players[s].position,
            label: v.players[s].name.isEmpty
                ? '${s + 1}'
                : v.players[s].name.characters.first.toUpperCase(),
          ),
    ];
  }

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
              child: _tile(i, cellW: w),
            ),
          // Animated pawns ride on top of the tiles.
          Positioned.fill(
            child: _PawnLayer(cellW: w, cellH: h, pawns: _pawns()),
          ),
        ]);
      }),
    );
  }

  // Non-40 boards (mods): plain wrap, the center panel goes below. Pawns are
  // drawn statically in-tile here (no ring geometry to glide along).
  Widget _wrapLayout() {
    return Column(children: [
      Wrap(
        spacing: 2,
        runSpacing: 2,
        children: [
          for (var i = 0; i < content.board.length; i++)
            SizedBox(
                width: 110, height: 96, child: _tile(i, cellW: 110, staticPawns: true)),
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

  Widget _tile(int i, {required double cellW, bool staticPawns = false}) {
    final def = content.board[i];
    final ts = view?.tiles.elementAtOrNull(i);
    // Text scales with the cell so it stays legible on any window size.
    final nameSize = (cellW * 0.115).clamp(11.0, 17.0);
    final metaSize = (cellW * 0.095).clamp(9.0, 13.0);
    final bandH = (cellW * 0.11).clamp(9.0, 18.0);
    final ownerSz = (cellW * 0.13).clamp(9.0, 16.0);
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
            Container(height: bandH, color: band),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(4, 3, 4, 0),
                child: Text(
                  def.name,
                  maxLines: 3,
                  style: TextStyle(
                    fontSize: nameSize,
                    height: 1.1,
                    fontWeight: FontWeight.w600,
                    color: const Color(0xFF1E1E1E),
                  ),
                  overflow: TextOverflow.fade,
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(4, 0, 4, 3),
              child: Text(
                _meta(def, ts),
                style: TextStyle(
                  fontSize: metaSize,
                  fontWeight: FontWeight.w700,
                  color: ts?.mortgaged == true
                      ? const Color(0xFFC0564F)
                      : const Color(0xFF555555),
                ),
              ),
            ),
          ]),
          if (ts?.owner != null)
            Positioned(
              top: 2,
              right: 2,
              child: Container(
                width: ownerSz,
                height: ownerSz,
                decoration: BoxDecoration(
                  color: pawnColors[ts!.owner! % pawnColors.length],
                  borderRadius: BorderRadius.circular(2),
                  border: Border.all(color: Colors.black26),
                ),
              ),
            ),
          if (staticPawns) _staticPawns(i),
        ]),
      ),
    );
  }

  Widget _staticPawns(int i) {
    final v = view;
    if (v == null) return const SizedBox.shrink();
    final here = [
      for (var s = 0; s < v.players.length; s++)
        if (!v.players[s].bankrupt && v.players[s].position == i) s,
    ];
    return Positioned(
      bottom: 3,
      left: 3,
      child: Row(children: [
        for (final s in here)
          Container(
            width: 16,
            height: 16,
            margin: const EdgeInsets.only(right: 3),
            decoration: BoxDecoration(
              color: pawnColors[s % pawnColors.length],
              shape: BoxShape.circle,
              border: Border.all(color: Colors.white, width: 1.5),
            ),
          ),
      ]),
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

class PawnData {
  final int seat;
  final Color color;
  final int position;
  final String label;
  const PawnData({
    required this.seat,
    required this.color,
    required this.position,
    required this.label,
  });
}

/// Overlay that draws each pawn and animates it when its tile changes.
/// A normal roll (short forward distance) hops tile by tile around the
/// ring; a teleport (card, jail, backward) slides straight to the target.
class _PawnLayer extends StatefulWidget {
  final double cellW, cellH;
  final List<PawnData> pawns;
  const _PawnLayer({required this.cellW, required this.cellH, required this.pawns});

  @override
  State<_PawnLayer> createState() => _PawnLayerState();
}

class _PawnAnim {
  final AnimationController ctrl;
  List<int> waypoints; // tile indices to glide through
  int target; // where the pawn currently rests / is heading
  _PawnAnim(this.ctrl, this.target) : waypoints = [target];
}

class _PawnLayerState extends State<_PawnLayer> with TickerProviderStateMixin {
  static const _boardLen = 40;
  final Map<int, _PawnAnim> _anims = {};

  @override
  void initState() {
    super.initState();
    for (final p in widget.pawns) {
      _anims[p.seat] = _PawnAnim(AnimationController(vsync: this), p.position);
    }
  }

  @override
  void didUpdateWidget(_PawnLayer old) {
    super.didUpdateWidget(old);
    for (final p in widget.pawns) {
      final a = _anims.putIfAbsent(
          p.seat, () => _PawnAnim(AnimationController(vsync: this), p.position));
      if (p.position != a.target) _animate(a, p.position);
    }
  }

  void _animate(_PawnAnim a, int to) {
    final from = a.target;
    final forward = (to - from) % _boardLen; // 0..39
    final List<int> path;
    if (forward >= 1 && forward <= 12) {
      // Dice-sized move: hop each tile so the pawn follows the border.
      path = [for (var k = 0; k <= forward; k++) (from + k) % _boardLen];
      a.ctrl.duration = Duration(milliseconds: (forward * 130).clamp(200, 1700));
    } else {
      // Teleport / backward / long jump: glide straight to the target.
      path = [from, to];
      a.ctrl.duration = const Duration(milliseconds: 500);
    }
    a.target = to;
    setState(() => a.waypoints = path);
    a.ctrl.forward(from: 0).whenComplete(() {
      if (mounted) setState(() => a.waypoints = [a.target]);
    });
  }

  Offset _center(int i) {
    final c = cellOf(i);
    return Offset((c.c - 0.5) * widget.cellW, (c.r - 0.5) * widget.cellH);
  }

  Offset _offsetOf(_PawnAnim a) {
    final pts = [for (final i in a.waypoints) _center(i)];
    if (pts.length == 1) return pts.first;
    final p = a.ctrl.value * (pts.length - 1);
    final seg = p.floor().clamp(0, pts.length - 2);
    return Offset.lerp(pts[seg], pts[seg + 1], p - seg)!;
  }

  @override
  Widget build(BuildContext context) {
    final size = (widget.cellW * 0.42).clamp(18.0, 34.0);
    final fanR = widget.cellW * 0.16;
    final controllers = [for (final a in _anims.values) a.ctrl];
    return IgnorePointer(
      child: AnimatedBuilder(
        animation: Listenable.merge(controllers),
        builder: (context, _) {
          return Stack(children: [
            for (final p in widget.pawns)
              if (_anims[p.seat] case final a?)
                _positioned(p, _offsetOf(a), size, fanR),
          ]);
        },
      ),
    );
  }

  // Fan pawns around the tile centre so several on one tile stay distinct.
  Widget _positioned(PawnData p, Offset c, double size, double fanR) {
    final angle = p.seat * (2 * math.pi / 6);
    final fan = Offset(math.cos(angle), math.sin(angle)) * fanR;
    return Positioned(
      left: c.dx + fan.dx - size / 2,
      top: c.dy + fan.dy - size / 2,
      width: size,
      height: size,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: p.color,
          shape: BoxShape.circle,
          border: Border.all(color: Colors.white, width: 2),
          boxShadow: const [
            BoxShadow(color: Colors.black45, blurRadius: 3, offset: Offset(0, 1)),
          ],
        ),
        child: Center(
          child: Text(
            p.label,
            style: TextStyle(
              color: Colors.white,
              fontWeight: FontWeight.bold,
              fontSize: size * 0.5,
            ),
          ),
        ),
      ),
    );
  }

  @override
  void dispose() {
    for (final a in _anims.values) {
      a.ctrl.dispose();
    }
    super.dispose();
  }
}
