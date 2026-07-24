/// Invariants of the pure isometric projection (DDR-0022, step A). The module
/// has no Flutter, no state and no board coupling, so these are ordinary unit
/// tests: determinism, exact invertibility on every board cell, injectivity
/// (no ambiguity), and preservation of grid-neighbour relations.
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:parcello_client/iso_projection.dart';

/// Every cell of a `d x d` grid, 1-based like the board's `d x d` ring layout
/// (the module itself is grid-size agnostic; the test picks the sizes).
Iterable<IsoCell> _grid(int d) sync* {
  for (var r = 1; r <= d; r++) {
    for (var c = 1; c <= d; c++) {
      yield IsoCell(r, c);
    }
  }
}

void main() {
  const proj = IsoProjection(); // canonical 2:1

  group('determinism and purity', () {
    test('same cell always projects to the same point', () {
      const cell = IsoCell(3, 5);
      expect(proj.project(cell), proj.project(cell));
    });

    test('equal cells project equally (value identity drives the map)', () {
      expect(proj.project(const IsoCell(2, 7)), proj.project(IsoCell(2, 7)));
    });

    test('a fresh projection with the same shape is equal and agrees', () {
      const other = IsoProjection();
      expect(other, proj);
      expect(other.hashCode, proj.hashCode);
      for (final cell in _grid(9)) {
        expect(other.project(cell), proj.project(cell));
      }
    });

    test('interleaving calls carries no state between them', () {
      // Projecting many other cells in between must not change a result.
      final before = proj.project(const IsoCell(4, 4));
      for (final cell in _grid(11)) {
        proj.project(cell);
        proj.unproject(proj.project(cell));
      }
      expect(proj.project(const IsoCell(4, 4)), before);
    });
  });

  group('forward projection', () {
    test('known 2:1 values', () {
      // x = (col - row) * 1.0 ; y = (col + row) * 0.5
      expect(proj.project(const IsoCell(1, 1)), const IsoPoint(0.0, 1.0));
      expect(proj.project(const IsoCell(1, 2)), const IsoPoint(1.0, 1.5));
      expect(proj.project(const IsoCell(2, 1)), const IsoPoint(-1.0, 1.5));
    });
  });

  group('inverse projection', () {
    test('unproject is the correct inverse of a known point', () {
      expect(proj.unproject(const IsoPoint(1.0, 1.5)), const IsoCell(1, 2));
      expect(proj.unproject(const IsoPoint(-1.0, 1.5)), const IsoCell(2, 1));
    });

    test('round-trip: unproject(project(cell)) == cell for every board cell',
        () {
      for (final d in const [8, 9, 11, 21]) {
        for (final cell in _grid(d)) {
          expect(proj.unproject(proj.project(cell)), cell,
              reason: 'round-trip failed for $cell on a ${d}x$d grid');
        }
      }
    });

    test('round-trip holds under a non-default diamond shape', () {
      const flat = IsoProjection(halfWidth: 0.5, halfHeight: 0.25);
      for (final cell in _grid(9)) {
        expect(flat.unproject(flat.project(cell)), cell);
      }
    });
  });

  group('no ambiguity (injectivity)', () {
    test('distinct cells never share a projected point', () {
      final seen = <IsoPoint>{};
      var count = 0;
      for (final cell in _grid(21)) {
        seen.add(proj.project(cell));
        count++;
      }
      expect(seen.length, count, reason: 'two cells collided onto one point');
    });

    test('every projected point maps back to exactly one cell', () {
      for (final cell in _grid(11)) {
        expect(proj.unproject(proj.project(cell)), cell);
      }
    });
  });

  group('topology: grid neighbours are preserved', () {
    test('the +col and +row steps are constant displacement vectors', () {
      const expectCol = IsoPoint(1.0, 0.5); // project(r, c+1) - project(r, c)
      const expectRow = IsoPoint(-1.0, 0.5); // project(r+1, c) - project(r, c)
      for (final cell in _grid(9)) {
        final here = proj.project(cell);
        final rightCol = proj.project(IsoCell(cell.row, cell.col + 1));
        final downRow = proj.project(IsoCell(cell.row + 1, cell.col));
        expect(IsoPoint(rightCol.x - here.x, rightCol.y - here.y), expectCol,
            reason: 'the +col step drifted at $cell');
        expect(IsoPoint(downRow.x - here.x, downRow.y - here.y), expectRow,
            reason: 'the +row step drifted at $cell');
      }
    });

    test('adjacent cells stay adjacent and distinct after projection', () {
      for (final cell in _grid(9)) {
        final here = proj.project(cell);
        final right = proj.project(IsoCell(cell.row, cell.col + 1));
        expect(here == right, isFalse); // neighbours never coincide
      }
    });
  });

  group('depth order (DDR-0022 step C3)', () {
    test('depth IS the projected y, not a second formula', () {
      for (final cell in _grid(9)) {
        expect(proj.depth(cell), proj.project(cell).y, reason: 'at $cell');
      }
      // And it follows the projection's shape rather than a hardcoded row+col.
      const flat = IsoProjection(halfHeight: 0.25);
      for (final cell in _grid(5)) {
        expect(flat.depth(cell), flat.project(cell).y, reason: 'at $cell');
      }
    });

    test('compareDepth agrees with row + col on the canonical 2:1 diamond', () {
      for (final a in _grid(9)) {
        for (final b in _grid(9)) {
          final sum = (a.row + a.col).compareTo(b.row + b.col);
          if (sum != 0) {
            expect(proj.compareDepth(a, b), sum,
                reason: '$a vs $b disagrees with row+col');
          }
        }
      }
    });

    test('compareDepth is a strict total order on every board cell', () {
      // Strict and total is what makes a sort deterministic: `List.sort` is not
      // stable, so a comparator returning 0 for two distinct cells would let
      // their paint order drift between runs.
      final cells = _grid(9).toList();
      for (final a in cells) {
        expect(proj.compareDepth(a, a), 0, reason: 'not reflexive at $a');
        for (final b in cells) {
          final ab = proj.compareDepth(a, b);
          final ba = proj.compareDepth(b, a);
          expect(ab.sign, -ba.sign, reason: 'not antisymmetric: $a vs $b');
          if (a != b) {
            expect(ab, isNot(0), reason: 'distinct cells $a and $b tie');
          }
        }
      }
      // Transitivity, via the property that makes it useful: sorting yields a
      // sequence that is strictly increasing under the comparator itself.
      final sorted = [...cells]..sort(proj.compareDepth);
      for (var i = 1; i < sorted.length; i++) {
        expect(proj.compareDepth(sorted[i - 1], sorted[i]), lessThan(0));
      }
    });

    test('cells at equal depth are 2 units apart, so their order is invisible',
        () {
      // Why the x tie-break is for determinism only: nothing narrower than two
      // projection units can occlude across an anti-diagonal.
      final cells = _grid(9).toList();
      var pairs = 0;
      for (var i = 0; i < cells.length; i++) {
        for (var j = i + 1; j < cells.length; j++) {
          if (proj.depth(cells[i]) != proj.depth(cells[j])) continue;
          pairs++;
          final dx =
              (proj.project(cells[i]).x - proj.project(cells[j]).x).abs();
          expect(dx, greaterThanOrEqualTo(2.0),
              reason: '${cells[i]} and ${cells[j]} share a depth and sit $dx '
                  'apart');
        }
      }
      expect(pairs, greaterThan(0), reason: 'no equal-depth pair was examined');
    });

    test('the order is invariant under any IsoFit (a camera cannot reorder it)',
        () {
      // Depth is a rank, and IsoFit is an affine transform with a positive
      // scale - so a resize, and later a zoom/pan camera or a board with a fixed
      // logical size, cannot change which cell is in front.
      final cells = _grid(9).toList();
      final byDepth = [...cells]..sort(proj.compareDepth);
      for (final fit in [
        proj.fit(cells, width: 320, height: 320),
        proj.fit(cells, width: 1000, height: 200),
        proj.fit(cells, width: 200, height: 1000, padding: 17),
        const IsoFit(scale: 0.01, offsetX: -900, offsetY: 4000),
      ]) {
        final byFittedPoint = [...cells]
          ..sort((a, b) {
            final pa = fit.apply(proj.project(a));
            final pb = fit.apply(proj.project(b));
            final dy = pa.y.compareTo(pb.y);
            return dy != 0 ? dy : pa.x.compareTo(pb.x);
          });
        expect(byFittedPoint, byDepth, reason: 'reordered by $fit');
      }
    });
  });

  group('bounds and fit (pure viewport maths, still no Flutter)', () {
    test('bounds of a d x d grid span its projected extremes', () {
      // 9x9, cells 1..9: x = (col-row) in [-8, 8], y = (col+row) in [2, 18].
      final b = proj.bounds(_grid(9));
      expect(b.minX, -8.0);
      expect(b.maxX, 8.0);
      expect(b.minY, 2.0 * 0.5); // (1+1)*halfHeight
      expect(b.maxY, 18.0 * 0.5); // (9+9)*halfHeight
      expect(b.width, 16.0);
      expect(b.height, 8.0);
    });

    test('fit scales the diamond uniformly and centres it in the box', () {
      final cells = _grid(9).toList();
      final fit = proj.fit(cells, width: 320, height: 320);
      // 2:1 diamond (16 wide, 8 tall) in a square box: width binds, scale=20.
      expect(fit.scale, closeTo(20.0, 1e-9));
      // Every projected cell lands inside the box after the fit.
      for (final cell in cells) {
        final p = fit.apply(proj.project(cell));
        expect(p.x, inInclusiveRange(0.0, 320.0));
        expect(p.y, inInclusiveRange(0.0, 320.0));
      }
      // The diamond centre maps to the box centre.
      final mid = fit.apply(proj.bounds(cells).center);
      expect(mid.x, closeTo(160.0, 1e-9));
      expect(mid.y, closeTo(160.0, 1e-9));
    });

    test('padding insets all sides and keeps points within the box', () {
      final cells = _grid(9).toList();
      final fit = proj.fit(cells, width: 320, height: 320, padding: 10);
      // width binds: scale = (320 - 20) / 16 = 18.75.
      expect(fit.scale, closeTo(18.75, 1e-9));
      for (final cell in cells) {
        final p = fit.apply(proj.project(cell));
        expect(p.x, inInclusiveRange(10.0, 310.0));
        expect(p.y, inInclusiveRange(10.0, 310.0));
      }
    });

    test('fit is deterministic and side-effect free', () {
      final cells = _grid(11).toList();
      final a = proj.fit(cells, width: 500, height: 400);
      final b = proj.fit(cells, width: 500, height: 400);
      expect(a, b);
      expect(a.hashCode, b.hashCode);
    });
  });

  group('value types', () {
    test('IsoCell equality and hashCode are structural', () {
      expect(const IsoCell(1, 2), const IsoCell(1, 2));
      expect(const IsoCell(1, 2).hashCode, const IsoCell(1, 2).hashCode);
      expect(const IsoCell(1, 2) == const IsoCell(2, 1), isFalse);
    });

    test('IsoPoint equality and hashCode are structural', () {
      expect(const IsoPoint(1.0, 2.0), const IsoPoint(1.0, 2.0));
      expect(const IsoPoint(1.0, 2.0).hashCode, const IsoPoint(1.0, 2.0).hashCode);
      expect(const IsoPoint(1.0, 2.0) == const IsoPoint(2.0, 1.0), isFalse);
    });
  });
}
