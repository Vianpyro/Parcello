/// Board rendering: a `4*(d-1)` square ring (32 -> 9x9, 40 -> 11x11), wrap
/// fallback for other modded sizes. A pure projection of content + view +
/// stage; taps bubble up.
///
/// The board is the protagonist (`docs/motion-language.md` 2): every animation
/// starts or ends on something drawn here, and the camera never moves.
/// Attention is expressed by exactly three devices - frame, lift, recede - and
/// nothing else.
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'motion.dart';
import 'protocol.dart';
import 'sfx.dart';
import 'stage.dart';
import 'tokens.dart';

/// A board of `n` tiles renders as a square ring when `n` is `4*(d-1)`
/// for a `d`x`d` grid (32 -> 9x9, 40 -> 11x11, ...).
bool isSquareRing(int n) => n >= 8 && n % 4 == 0;
int ringSide(int n) => n ~/ 4 + 1; // the `d` above

/// Grid cell (1-based row/col) of tile `i` on a `d`x`d` ring of `4*(d-1)`
/// tiles: 0 is the bottom-right corner, walking counter-clockwise.
({int r, int c}) cellOf(int i, int d) {
  if (i <= d - 1) return (r: d, c: d - i); // bottom row (d tiles)
  if (i <= 2 * d - 2) return (r: 2 * d - 1 - i, c: 1); // left column up
  if (i <= 3 * d - 3) return (r: 1, c: i - 2 * d + 3); // top row
  return (r: i - 3 * d + 4, c: d); // right column down
}

class BoardWidget extends StatefulWidget {
  final GameContent content;
  final ClientView? view;
  final int? mySeat;
  final void Function(int tile) onTileTap;

  /// Whether tapping this tile would actually offer an action - gates the
  /// hover outline (no highlight promising something a tap cannot do).
  final bool Function(int tile) canAct;

  /// What the board is currently *showing*: pawn positions mid-chain, the
  /// focused tile, the recede, bands in the middle of changing hands.
  final StageState stage;

  /// Tile to outline as a movement destination (hovered hand card), if any.
  final int? highlightTile;
  final Widget center;

  const BoardWidget({
    super.key,
    required this.content,
    required this.view,
    required this.mySeat,
    required this.onTileTap,
    required this.canAct,
    required this.stage,
    this.highlightTile,
    required this.center,
  });

  @override
  State<BoardWidget> createState() => _BoardWidgetState();
}

class _BoardWidgetState extends State<BoardWidget> {
  /// Tile currently under the pointer. A plain field (not StatefulBuilder-local)
  /// so it survives rebuilds triggered by a server Update while the mouse has
  /// not moved.
  int? _hoveredTile;

  /// Tile currently holding keyboard/controller focus, so a gamepad / Steam
  /// Deck player can see which actionable tile is selected before pressing A.
  /// Only actionable tiles (canAct) are ever focusable, so the D-pad traverses
  /// just those rather than all 32 squares.
  int? _focusedTile;

  final _ringKey = GlobalKey();

  List<PawnData> _pawns() {
    final v = widget.view;
    if (v == null) return const [];
    return [
      for (var s = 0; s < v.players.length; s++)
        if (!v.players[s].bankrupt)
          PawnData(
            seat: s,
            color: pawnColor(s),
            position: widget.stage.pawnAt[s] ?? v.players[s].position,
            label: v.players[s].name.isEmpty
                ? '${s + 1}'
                : v.players[s].name.characters.first.toUpperCase(),
          ),
    ];
  }

  /// Hands the stage a way to turn a tile index into a screen point, so a chit
  /// can fly from a board tile to a seat marker in the side panel. Only this
  /// widget knows the ring geometry.
  void _installAnchors(int d, double w, double h) {
    widget.stage.anchors.tiles = (tile) {
      final box = _ringKey.currentContext?.findRenderObject() as RenderBox?;
      if (box == null || !box.hasSize || tile < 0) return null;
      final c = cellOf(tile, d);
      return box.localToGlobal(Offset((c.c - 0.5) * w, (c.r - 0.5) * h));
    };
  }

  /// The board subscribes to the stage itself rather than relying on a caller
  /// to wrap it - a component handed a notifier should listen to it, and one
  /// that quietly does not is a footgun (it renders a stale frame and nothing
  /// says why).
  ///
  /// `center` is a widget built by the *caller*, so on an animation frame it is
  /// the same instance and Flutter reuses its element untouched. That is what
  /// keeps the action panel's text fields out of the repaint path: the board
  /// repaints forty times a second, the half-typed bid inside it does not.
  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.stage,
        builder: (context, _) => _build(context),
      );

  Widget _build(BuildContext context) {
    final n = widget.content.board.length;
    if (!isSquareRing(n)) return _wrapLayout();
    final d = ringSide(n); // grid is d x d
    return AspectRatio(
      aspectRatio: 1,
      child: LayoutBuilder(builder: (context, box) {
        final w = box.maxWidth / d, h = box.maxHeight / d;
        _installAnchors(d, w, h);
        return Stack(key: _ringKey, children: [
          Positioned(
            left: w,
            top: h,
            width: w * (d - 2),
            height: h * (d - 2),
            child: Container(
              margin: const EdgeInsets.all(Pc.s2),
              padding: const EdgeInsets.all(Pc.s8),
              // The centre plaza (visual-identity.md), not a white sheet.
              color: Pc.sage,
              child: widget.center,
            ),
          ),
          for (var i = 0; i < n; i++)
            Positioned(
              left: (cellOf(i, d).c - 1) * w,
              top: (cellOf(i, d).r - 1) * h,
              width: w,
              height: h,
              child: _tile(i, cellW: w),
            ),
          Positioned.fill(
            child: _PawnLayer(
                side: d, cellW: w, cellH: h, pawns: _pawns(), stage: widget.stage),
          ),
        ]);
      }),
    );
  }

  // Non-ring boards (mods): plain wrap, the centre panel goes below. Pawns are
  // drawn statically in-tile here (no ring geometry to glide along).
  Widget _wrapLayout() {
    return Column(children: [
      Wrap(
        spacing: 2,
        runSpacing: 2,
        children: [
          for (var i = 0; i < widget.content.board.length; i++)
            SizedBox(
                width: 110,
                height: 96,
                child: _tile(i, cellW: 110, staticPawns: true)),
        ],
      ),
      const SizedBox(height: Pc.s8),
      Expanded(
        child: Container(
          padding: const EdgeInsets.all(Pc.s8),
          color: Pc.sage,
          child: widget.center,
        ),
      ),
    ]);
  }

  /// Whether `owner` holds every tile of `group` - drives the "SET" badge (the
  /// unimproved x2 was invisible before).
  bool _ownsFullGroup(int owner, String? group) {
    if (group == null) return false;
    final v = widget.view;
    if (v == null) return false;
    final b = widget.content.board;
    for (var i = 0; i < b.length && i < v.tiles.length; i++) {
      if (b[i].isProperty && b[i].group == group && v.tiles[i].owner != owner) {
        return false;
      }
    }
    return true;
  }

  Widget _tile(int i, {required double cellW, bool staticPawns = false}) {
    final st = widget.stage;
    final def = widget.content.board[i];
    final ts = widget.view?.tiles.elementAtOrNull(i);
    final spotlit = widget.view?.spotlight?.tile == i;
    final hovering = _hoveredTile == i && widget.canAct(i);
    final focused = _focusedTile == i;
    final dest = widget.highlightTile == i;
    final fullGroup = ts?.owner != null && _ownsFullGroup(ts!.owner!, def.group);

    // The three attention devices, and nothing else (motion-language.md 2).
    final lifted = st.focusTile == i; // "act on this tile"
    final framed = st.frameTile == i; // "this tile is the subject"
    final receded = st.recede && !lifted; // "nothing else matters right now"
    final threatened = st.threatTiles.contains(i); // something was done to it

    // Ownership is the band. A tile changing hands sweeps its band to the new
    // owner's colour; until the sweep lands, the view still says the old owner.
    final sweepTo = st.sweeping[i];
    final owner = sweepTo ?? ts?.owner;

    // What this property costs to take right now, if a market event is moving
    // prices; null otherwise.
    final marketPrice = _marketPrice(def);

    final nameSize = (cellW * 0.115).clamp(11.0, 17.0);
    final metaSize = (cellW * 0.095).clamp(9.0, 13.0);
    final bandH = (cellW * 0.11).clamp(9.0, 18.0);
    final ownerSz = (cellW * 0.13).clamp(9.0, 16.0);

    // The band is the group colour; a full gold band marks the group-scaled
    // "utility" tiles. Non-properties get no band at all.
    final band = def.isProperty
        ? (groupColors[def.group] ?? Pc.textFaint)
        : Pc.borderMuted;

    return MouseRegion(
      onEnter: (_) => setState(() => _hoveredTile = i),
      onExit: (_) {
        // Guard against an adjacent tile's onEnter firing before this tile's
        // onExit - only clear if we are still the hovered one.
        if (_hoveredTile == i) setState(() => _hoveredTile = null);
      },
      child: _Refusable(
        // "No" is a physical gesture, and it belongs on the thing that said it.
        // The only lateral shake in the game.
        active: st.refuseTile == i,
        seq: st.refuseSeq,
        child: AnimatedOpacity(
        opacity: receded ? 0.35 : 1,
        duration: Motion.establish,
        curve: Motion.deliberate,
        child: AnimatedScale(
          scale: lifted ? 1.06 : 1,
          duration: Motion.establish,
          curve: Motion.deliberate,
          child: Container(
            decoration: BoxDecoration(
              border: dest
                  ? Border.all(color: Pc.gold, width: 3)
                  : hovering
                      ? Border.all(color: Pc.gold, width: 1.5)
                      : focused
                          ? Border.all(color: Pc.gold, width: Pc.s2)
                          : null,
              borderRadius: Pc.radius,
            ),
            // Focusable only when actionable, so a controller / Steam Deck
            // D-pad steps through the tiles the player can act on and A
            // (Enter/Space -> ActivateIntent) opens the same tile menu as a
            // tap. GestureDetector still handles the pointer path.
            child: FocusableActionDetector(
              enabled: widget.canAct(i),
              onShowFocusHighlight: (f) {
                if (f) {
                  setState(() => _focusedTile = i);
                } else if (_focusedTile == i) {
                  setState(() => _focusedTile = null);
                }
              },
              actions: {
                ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
                  widget.onTileTap(i);
                  return null;
                }),
              },
              child: GestureDetector(
                onTap: () => widget.onTileTap(i),
                child: AnimatedContainer(
                duration: Motion.refuse,
                curve: Motion.threat, // snaps in, lingers
                margin: const EdgeInsets.all(1),
                decoration: BoxDecoration(
                  // Property faces are card-stock parchment, never white.
                  color: threatened
                      ? Pc.oxblood
                      : def.isProperty
                          ? Pc.parchment
                          : Pc.surface,
                  borderRadius: Pc.radius,
                  border: Border.all(
                    color: lifted
                        ? Pc.gold
                        : spotlit
                            ? Pc.gold
                            : framed
                                ? Pc.goldDark
                                : owner != null && owner == widget.mySeat
                                    ? Pc.sage
                                    : Colors.transparent,
                    width: lifted || spotlit ? 3 : 2,
                  ),
                  boxShadow: lifted ? Pc.hairShadow : null,
                ),
                child: Stack(children: [
                  Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        // The band sweeps to the new owner's colour on a
                        // transfer; ownership is never announced in a popup.
                        AnimatedContainer(
                          duration: Motion.bandSweep,
                          curve: Motion.arrive,
                          height: bandH,
                          color: owner != null && def.isProperty
                              ? Color.lerp(band, pawnColor(owner), 0.55)!
                              : band,
                        ),
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
                                color: def.isProperty
                                    ? Pc.parchmentInk
                                    : Pc.text,
                              ),
                              overflow: TextOverflow.fade,
                            ),
                          ),
                        ),
                        if (ts != null && ts.houses > 0)
                          Padding(
                            padding: const EdgeInsets.only(left: 3),
                            child: Row(children: [
                              if (ts.houses >= 5)
                                Icon(Icons.apartment,
                                    size: metaSize + 5, color: Pc.sage)
                              else
                                for (var h = 0; h < ts.houses; h++)
                                  Icon(Icons.home,
                                      size: metaSize + 3, color: Pc.sage),
                            ]),
                          ),
                        Padding(
                          padding: const EdgeInsets.fromLTRB(4, 0, 4, 3),
                          child: Text(
                            _meta(def, ts,
                                spotlit: spotlit, fullGroup: fullGroup),
                            style: TextStyle(
                              fontSize: metaSize,
                              fontWeight: FontWeight.w700,
                              color: ts?.mortgaged == true
                                  ? Pc.oxblood
                                  : spotlit
                                      ? Pc.goldDark
                                      // A market event moving the price says so
                                      // in the grammar the player already knows:
                                      // cheaper to take reads as a gain, dearer
                                      // as a loss.
                                      : marketPrice != null
                                          ? (marketPrice < (def.price ?? 0)
                                              ? Pc.gainInk
                                              : Pc.lossInk)
                                          : def.isProperty
                                              ? Pc.textFaint
                                              : Pc.textMuted,
                            ),
                          ),
                        ),
                      ]),
                  if (spotlit)
                    const Positioned(
                        top: Pc.s2,
                        left: Pc.s2,
                        child: Icon(Icons.auto_awesome,
                            size: 12, color: Pc.goldDark)),
                  if (owner != null)
                    Positioned(
                      top: Pc.s2,
                      right: Pc.s2,
                      child: AnimatedContainer(
                        duration: Motion.bandSweep,
                        curve: Motion.arrive,
                        width: ownerSz,
                        height: ownerSz,
                        decoration: BoxDecoration(
                          color: pawnColor(owner),
                          borderRadius: Pc.radius,
                          border: Border.all(color: Pc.parchmentInk, width: 0.5),
                        ),
                      ),
                    ),
                  if (staticPawns) _staticPawns(i),
                ]),
              ),
            ),
            ),
          ),
        ),
        ),
      ),
    );
  }

  Widget _staticPawns(int i) {
    final v = widget.view;
    if (v == null) return const SizedBox.shrink();
    final here = [
      for (var s = 0; s < v.players.length; s++)
        if (!v.players[s].bankrupt &&
            (widget.stage.pawnAt[s] ?? v.players[s].position) == i)
          s,
    ];
    return Positioned(
      bottom: 3,
      left: 3,
      child: Row(children: [
        for (final s in here)
          Container(
            width: Pc.s16,
            height: Pc.s16,
            margin: const EdgeInsets.only(right: 3),
            decoration: BoxDecoration(
              color: pawnColor(s),
              shape: BoxShape.circle,
              border: Border.all(color: Pc.parchment, width: 1.5),
            ),
          ),
      ]),
    );
  }

  /// What a property actually costs to take *right now*, or null when no market
  /// event is moving prices.
  ///
  /// An `acquisition_multiplier` (the base mod's Market Bubble) scales what a
  /// sealed-bid winner settles at and what a takeover costs - so while it is
  /// active, the list price printed on the tile is simply not the price. The
  /// forecast strip promised a consequence three turns ago; the board is where
  /// that promise has to be kept. Mirrors the engine's `apply_market_multiplier`
  /// exactly, including its truncating division.
  /// The current price when the market is actually moving it, `null` when it
  /// is not - the caller colours a moved price as a gain or a loss, so "no
  /// change" has to be distinguishable from "unchanged number".
  int? _marketPrice(TileDef def) {
    if (!def.isProperty) return null;
    final now = marketPrice(def, widget.view);
    return now == (def.price ?? 0) ? null : now;
  }

  String _meta(
    TileDef def,
    TileState? ts, {
    bool spotlit = false,
    bool fullGroup = false,
  }) {
    final parts = <String>[];
    if (def.isProperty) {
      final market = _marketPrice(def);
      // The list price stays visible next to it: a player must be able to see
      // that the number moved, and by how much, not just that it is different
      // from the one they memorised.
      parts.add(market == null
          ? '\$${def.price}'
          : '\$$market (was \$${def.price})');
    }
    if (def.amount != null) parts.add('pay \$${def.amount}');
    if (def.minPct != null) parts.add('${def.minPct}-${def.maxPct}% NW');
    // The classic rule doubles UNIMPROVED rent only - surface it, and keep a
    // plain marker once houses take over the escalation.
    if (fullGroup) parts.add((ts?.houses ?? 0) == 0 ? 'SET x2' : 'SET');
    if (ts != null && ts.boosts > 0) parts.add('BOOST ${ts.boosts}');
    if (ts?.mortgaged == true) parts.add('MORT.');
    if (spotlit) parts.add('SPOTLIGHT');
    return parts.join('  ');
  }
}

/// Shakes its child once when the server refuses a command about it.
///
/// The only lateral shake in Parcello. Everywhere else motion resolves and
/// stops - but "no" is a physical gesture, and an error that appears in a log
/// the player is not reading, or in a modal that interrupts them, is an error
/// they have to *hunt* for. It belongs on the thing that said it.
class _Refusable extends StatefulWidget {
  final bool active;
  final int seq;
  final Widget child;
  const _Refusable(
      {required this.active, required this.seq, required this.child});

  @override
  State<_Refusable> createState() => _RefusableState();
}

class _RefusableState extends State<_Refusable>
    with SingleTickerProviderStateMixin {
  // preserve: the app's MotionProfile (ADR-0030) is the sole motion authority.
  // Without it, a platform `disableAnimations` flag (Flutter Web sets it from
  // the browser's reduced-motion) silently scales every controller to 0.05x,
  // overriding the profile - the user could not get full motion even by asking.
  late final _ctrl = AnimationController(
      vsync: this,
      duration: Motion.refuse,
      animationBehavior: AnimationBehavior.preserve);

  @override
  void didUpdateWidget(_Refusable old) {
    super.didUpdateWidget(old);
    if (widget.active && widget.seq != old.seq) _ctrl.forward(from: 0);
  }

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: _ctrl,
        child: widget.child,
        builder: (context, child) => Transform.translate(
          // Three shakes, damped to nothing: a refusal, not a tantrum.
          offset: Offset(
              math.sin(_ctrl.value * math.pi * 6) * 3 * (1 - _ctrl.value), 0),
          child: child,
        ),
      );
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
///
/// A normal move hops tile by tile - the count is the information, and a player
/// should be able to count it. A teleport slides straight. Which of the two a
/// given move gets is decided by the director (the truth rule: motion may not
/// imply a path the engine did not take), and handed over on the stage.
class _PawnLayer extends StatefulWidget {
  final int side; // ring grid dimension (d)
  final double cellW, cellH;
  final List<PawnData> pawns;
  final StageState stage;
  const _PawnLayer({
    required this.side,
    required this.cellW,
    required this.cellH,
    required this.pawns,
    required this.stage,
  });

  @override
  State<_PawnLayer> createState() => _PawnLayerState();
}

class _PawnAnim {
  final AnimationController ctrl;
  List<int> waypoints; // tile indices to glide through
  int target; // where the pawn currently rests / is heading
  int lastHopSeg = 0; // highest tile-step already sounded this move
  _PawnAnim(this.ctrl, this.target) : waypoints = [target];
}

class _PawnLayerState extends State<_PawnLayer> with TickerProviderStateMixin {
  int get _boardLen => 4 * (widget.side - 1);
  final Map<int, _PawnAnim> _anims = {};

  _PawnAnim _makeAnim(int pos) {
    // preserve: keep the app's MotionProfile authoritative over the platform's
    // reduced-motion flag (see _RefusableState._ctrl).
    final anim = _PawnAnim(
        AnimationController(
            vsync: this, animationBehavior: AnimationBehavior.preserve),
        pos);
    anim.ctrl.addListener(() => _onTick(anim));
    anim.ctrl.addStatusListener((s) {
      if (s == AnimationStatus.completed) sfx.pawnStop();
    });
    return anim;
  }

  void _onTick(_PawnAnim a) {
    final segs = a.waypoints.length - 1;
    if (segs < 2) return; // teleport / single hop: only the landing sounds
    final seg = (a.ctrl.value * segs).floor();
    if (seg > a.lastHopSeg && seg < segs) {
      a.lastHopSeg = seg;
      sfx.moveHop(seg);
    }
  }

  @override
  void initState() {
    super.initState();
    for (final p in widget.pawns) {
      _anims[p.seat] = _makeAnim(p.position);
    }
  }

  @override
  void didUpdateWidget(_PawnLayer old) {
    super.didUpdateWidget(old);
    for (final p in widget.pawns) {
      final a = _anims.putIfAbsent(p.seat, () => _makeAnim(p.position));
      if (p.position != a.target) _animate(a, p.seat, p.position);
    }
  }

  void _animate(_PawnAnim a, int seat, int to) {
    final st = widget.stage;
    final from = a.target;
    a.lastHopSeg = 0;
    final forward = (to - from) % _boardLen;
    final straight = st.glide[seat] == true;
    final forceHop = st.forceHop[seat] == true;

    // The single source of truth for how long this takes is `Motion` - the
    // director costed the beat from the same constants. They used to be derived
    // independently and could silently drift.
    final hops = !straight &&
        (forceHop || (forward >= 1 && forward <= Motion.hopTaperFrom));
    final base = hops ? Motion.hop(forward) : Motion.glide;
    final scaled = base * st.hopScale;

    final path = hops
        ? [for (var k = 0; k <= forward; k++) (from + k) % _boardLen]
        : [from, to];

    a.ctrl.duration = scaled;
    a.target = to;
    // Reset now so the pawn holds at its START square during the wind-up
    // (otherwise it lingers at the previous move's end = 1.0).
    a.ctrl.reset();
    setState(() => a.waypoints = path);

    if (scaled == Duration.zero) {
      // Instant profile, or a beat the budget compressor zeroed: snap. The
      // state is never lost, only its journey.
      setState(() => a.waypoints = [to]);
      return;
    }

    // A beat between the command landing and the pawn setting off, so the move
    // reads as a decision followed by a consequence rather than one blur.
    Future.delayed(Motion.hopWindUp * st.hopScale, () {
      if (!mounted || a.target != to) return; // superseded by a newer move
      a.ctrl.forward(from: 0).whenComplete(() {
        if (mounted) setState(() => a.waypoints = [a.target]);
      });
    });
  }

  Offset _center(int i) {
    final c = cellOf(i, widget.side);
    return Offset((c.c - 0.5) * widget.cellW, (c.r - 0.5) * widget.cellH);
  }

  Offset _offsetOf(_PawnAnim a) {
    final pts = [for (final i in a.waypoints) _center(i)];
    if (pts.length == 1) return pts.first;
    final p = a.ctrl.value * (pts.length - 1);
    final seg = p.floor().clamp(0, pts.length - 2);
    // Ease within each tile-to-tile segment so the pawn hops from square to
    // square instead of gliding at constant speed. No bounce: motion resolves
    // and stops.
    final eased = Motion.arrive.transform(p - seg);
    return Offset.lerp(pts[seg], pts[seg + 1], eased)!;
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
          border: Border.all(color: Pc.text, width: Pc.s2),
          boxShadow: Pc.hairShadow,
        ),
        child: Center(
          child: Text(
            p.label,
            style: TextStyle(
              color: Pc.text,
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
