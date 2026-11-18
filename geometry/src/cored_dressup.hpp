// SPDX-License-Identifier: MIT
#pragma once

// Fillets whose rounded corners meet with none of an edge left between them
// (mitcad#1954): a cube of 10 rounded by 5 on every edge, a block of
// 10 x 10 x 20 rounded so, a stepped block rounded by half its width.
//
// OCCT's fillet cannot build them: the corner patches at both ends of such
// an edge meet on a spine of no length. Where the body is a prism of planar
// faces (two parallel ends, every other face a plane along the direction
// between them, reaching from end to end), the fillet's result is the set of
// points within r of the body's core, the section's polygon with every side
// the fillet rounds moved inward by r, between the ends moved inward by r:
// a rounding is the ball rolled along the core's edge. The core's corners
// are mitred (the moved sides meet where their lines cross), as the
// kernel's roundings of two edges that meet at an inner (concave) corner of
// a face end on each other's surfaces; a side the fillet leaves alone (no
// rounded edge on it) is moved outward instead, so that the result runs on
// past it and is cut by it. Where the ends lie exactly 2r apart, the core
// is flat; where the polygon's sides meet in a point or a segment, it is a
// point (the ball) or a segment (a capsule).
//
// Built here as the union of the core's pieces swept by the ball: the core
// polygon thickened by r (where it has an area), each core edge's half
// stadium (the ball's section about it, on its outer side) swept along it,
// and at each outer corner of the core the same section turned about the
// corner by the angle the core turns there; then cut by the faces the
// fillet leaves alone.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

struct CoredFillet {
  // The body rounded, its roundings named <feature>:fillet(<edge>) and its
  // corners <feature>:corner(<vertex>) (a face that several edges' or
  // corners' roundings became, the core's corner or edge they all meet in,
  // holds all their names), the faces left of the body's keeping theirs.
  // Null when the body or the selection is not such (see below).
  ShapePtr shape;
  // Selected inner (concave) edges between two rounded faces: rounded, they
  // would need the core's corner to be a rounding of its own, which this
  // construction does not build.
  std::vector<int> inner;
};

// The body's selected edges (`radius` per edge of the body, above 0 for
// those selected) rounded so. Null unless: one solid of planar faces, a
// prism along a selected straight edge's direction with its ends at least
// 2r apart and one loop of edges on each end; every selected edge of one
// radius r; the faces with no selected edge (left alone) are sides of the
// prism; every outer edge between two faces that have selected edges is
// selected, no inner one; the moved sides' lines meet in the polygon's
// order, no side turned round, and every corner of the core is r or more
// from every rounded side; the result is one valid solid.
CoredFillet cored_fillet(const std::string& feature, const Shape& body, const std::vector<double>& radius);

} // namespace mitcad::geometry::detail
