/// Pure isometric projection maths for a square grid (DDR-0022, step A):
/// foundations only. It maps an abstract `(row, col)` grid cell to a point in
/// a 2:1 diamond isometric space, and back - nothing else.
///
/// Deliberately self-contained: no Flutter, no `dart:ui`, no state, and no
/// coupling to the board's ring topology or its on-screen size. Whatever walks
/// the ring (`board.dart`'s `cellOf`) and whatever renders the result live
/// elsewhere; this file is only the geometry, so a later re-anchoring of the
/// board is a drop-in swap rather than a guess.
///
/// It also owns the DEPTH ORDER ([IsoProjection.depth] /
/// [IsoProjection.compareDepth]), for the same reason: which cell is in front of
/// which is a property of the projection, so it is defined once here rather than
/// re-derived by whatever paints, hit-tests or later stacks something on a cell.
///
/// The projection is a standard axonometric 2:1 diamond. Two properties are
/// what make it a safe foundation, and both are proven in
/// `test/iso_projection_test.dart`: it is injective (distinct cells never share
/// a point) and exactly invertible on grid cells (`unproject(project(cell)) ==
/// cell`). The bounds/fit helpers below keep the renderer from ever recomputing
/// a projection formula of its own: it consumes points from here, it does not
/// derive them.
library;

import 'dart:math' as math;

/// An abstract grid cell: integer `(row, col)`. No board, no pixels.
class IsoCell {
  final int row;
  final int col;
  const IsoCell(this.row, this.col);

  @override
  bool operator ==(Object other) =>
      other is IsoCell && other.row == row && other.col == col;

  @override
  int get hashCode => Object.hash(row, col);

  @override
  String toString() => 'IsoCell($row, $col)';
}

/// A point in isometric projection space. Unitless: a renderer scales and
/// offsets it into screen pixels, this module never does.
class IsoPoint {
  final double x;
  final double y;
  const IsoPoint(this.x, this.y);

  @override
  bool operator ==(Object other) =>
      other is IsoPoint && other.x == x && other.y == y;

  @override
  int get hashCode => Object.hash(x, y);

  @override
  String toString() => 'IsoPoint($x, $y)';
}

/// The axis-aligned bounding box of a set of projected points.
class IsoBounds {
  final double minX;
  final double minY;
  final double maxX;
  final double maxY;
  const IsoBounds({
    required this.minX,
    required this.minY,
    required this.maxX,
    required this.maxY,
  });

  double get width => maxX - minX;
  double get height => maxY - minY;
  IsoPoint get center => IsoPoint((minX + maxX) / 2, (minY + maxY) / 2);

  @override
  bool operator ==(Object other) =>
      other is IsoBounds &&
      other.minX == minX &&
      other.minY == minY &&
      other.maxX == maxX &&
      other.maxY == maxY;

  @override
  int get hashCode => Object.hash(minX, minY, maxX, maxY);

  @override
  String toString() => 'IsoBounds($minX, $minY, $maxX, $maxY)';
}

/// A uniform scale + translation that maps projection space into a pixel box.
/// Produced by [IsoProjection.fit] and applied at the render site: it is the
/// one place a projected point becomes a pixel point, and it does no isometric
/// maths of its own - only the affine viewport transform.
class IsoFit {
  final double scale;
  final double offsetX;
  final double offsetY;
  const IsoFit({
    required this.scale,
    required this.offsetX,
    required this.offsetY,
  });

  IsoPoint apply(IsoPoint p) =>
      IsoPoint(p.x * scale + offsetX, p.y * scale + offsetY);

  @override
  bool operator ==(Object other) =>
      other is IsoFit &&
      other.scale == scale &&
      other.offsetX == offsetX &&
      other.offsetY == offsetY;

  @override
  int get hashCode => Object.hash(scale, offsetX, offsetY);

  @override
  String toString() => 'IsoFit(scale: $scale, offsetX: $offsetX, '
      'offsetY: $offsetY)';
}

/// A pure, immutable 2:1 diamond isometric projection.
///
/// [halfWidth]/[halfHeight] are the diamond's unit half-extents (default 2:1).
/// They describe the projection's SHAPE, not the board's on-screen size. Two
/// projections with equal extents are themselves equal and produce identical
/// points: determinism is structural, there is no hidden state to drift.
class IsoProjection {
  final double halfWidth;
  final double halfHeight;

  const IsoProjection({this.halfWidth = 1.0, this.halfHeight = 0.5});

  /// Grid cell -> isometric point. Distinct cells always yield distinct points
  /// (the map is injective), so a projected board is never ambiguous.
  IsoPoint project(IsoCell cell) => IsoPoint(
        (cell.col - cell.row) * halfWidth,
        (cell.col + cell.row) * halfHeight,
      );

  /// Isometric point -> the single nearest grid cell. Exact for any point
  /// [project] produced (`unproject(project(cell)) == cell` for every cell);
  /// for an arbitrary point it returns the one closest cell, so there is
  /// exactly one cell per point and no seam of ambiguity between diamonds.
  IsoCell unproject(IsoPoint p) {
    final diff = p.x / halfWidth; // == col - row
    final sum = p.y / halfHeight; // == col + row
    return IsoCell(
      ((sum - diff) / 2).round(),
      ((sum + diff) / 2).round(),
    );
  }

  /// How close [cell] is to the eye: its projected `y`, and nothing else.
  ///
  /// Depth is not a separate model - in an axonometric projection the screen's
  /// vertical axis IS the view axis, so a cell can only occlude another one by
  /// sitting lower on screen. Two consequences worth stating, because both are
  /// easy to lose:
  ///
  /// - It is defined as `project(cell).y`, never as `row + col`. `row + col` is
  ///   only what that happens to be for a 2:1 diamond; deriving depth from the
  ///   projection means a change of [halfHeight], of anchoring (which corner is
  ///   nearest), or a mirrored board keeps the depth correct for free, whereas a
  ///   hand-written `row + col` at a use site silently would not.
  /// - It is a RANK, so it survives every affine viewport transform with a
  ///   positive scale - [IsoFit], and later a zoom/pan camera or a board with a
  ///   fixed logical size. Resizing the board cannot reorder it. What WOULD
  ///   invalidate it is a rotation: if the view ever rotates, the rotation
  ///   belongs in the projection (so `y` follows it), not in the transform
  ///   applied to projected points.
  double depth(IsoCell cell) => project(cell).y;

  /// Back-to-front order: sort cells by this and paint them in that order, and
  /// nearer cells land on top of farther ones (the painter's algorithm).
  ///
  /// [depth] first, then projected `x`. The `x` tie-break is there for
  /// determinism, not for looks: cells at equal depth are two projection units
  /// apart in `x` (one step along an anti-diagonal moves `x` by 2 and `y` by 0),
  /// so as long as what is drawn per cell is narrower than that, cells of equal
  /// depth cannot overlap and their relative order is invisible. Determinism
  /// still matters, because `List.sort` is not stable and a comparator that
  /// returns 0 for distinct cells would let their order drift between runs.
  ///
  /// It is a strict total order on cells, which is a consequence of [project]
  /// being injective (proven in `test/iso_projection_test.dart`): distinct cells
  /// never share a point, so they never share both `y` and `x`.
  int compareDepth(IsoCell a, IsoCell b) {
    final pa = project(a);
    final pb = project(b);
    final byDepth = pa.y.compareTo(pb.y);
    return byDepth != 0 ? byDepth : pa.x.compareTo(pb.x);
  }

  /// The bounding box of [cells] once projected.
  IsoBounds bounds(Iterable<IsoCell> cells) {
    var minX = double.infinity;
    var minY = double.infinity;
    var maxX = double.negativeInfinity;
    var maxY = double.negativeInfinity;
    for (final cell in cells) {
      final p = project(cell);
      if (p.x < minX) minX = p.x;
      if (p.y < minY) minY = p.y;
      if (p.x > maxX) maxX = p.x;
      if (p.y > maxY) maxY = p.y;
    }
    return IsoBounds(minX: minX, minY: minY, maxX: maxX, maxY: maxY);
  }

  /// The [IsoFit] that scales [cells]' projected bounds to fit a `width` x
  /// `height` pixel box uniformly (aspect preserved) and centres them in it.
  /// [padding] insets all four sides first, so a caller can reserve half a tile
  /// of margin and keep the diamond's vertex tiles fully inside the box.
  IsoFit fit(
    Iterable<IsoCell> cells, {
    required double width,
    required double height,
    double padding = 0,
  }) {
    final b = bounds(cells);
    final availW = width - 2 * padding;
    final availH = height - 2 * padding;
    final scale = math.min(
      b.width == 0 ? double.infinity : availW / b.width,
      b.height == 0 ? double.infinity : availH / b.height,
    );
    final offsetX = padding + (availW - b.width * scale) / 2 - b.minX * scale;
    final offsetY = padding + (availH - b.height * scale) / 2 - b.minY * scale;
    return IsoFit(scale: scale, offsetX: offsetX, offsetY: offsetY);
  }

  @override
  bool operator ==(Object other) =>
      other is IsoProjection &&
      other.halfWidth == halfWidth &&
      other.halfHeight == halfHeight;

  @override
  int get hashCode => Object.hash(halfWidth, halfHeight);

  @override
  String toString() => 'IsoProjection(halfWidth: $halfWidth, '
      'halfHeight: $halfHeight)';
}
