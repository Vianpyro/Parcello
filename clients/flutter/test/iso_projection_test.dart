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
