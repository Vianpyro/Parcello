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
/// The projection is a standard axonometric 2:1 diamond, so it needs only
/// arithmetic - not even `dart:math`. Two properties are what make it a safe
/// foundation, and both are proven in `test/iso_projection_test.dart`: it is
/// injective (distinct cells never share a point) and exactly invertible on
/// grid cells (`unproject(project(cell)) == cell`).
library;

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
