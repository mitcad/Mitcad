// SPDX-License-Identifier: MIT
#pragma once

// Boundary Fill (mitcad#457): the cells that tool bodies enclose. The
// tools' faces divide space into cells, each inside some of the tools and
// outside the others; a point inside a cell picks it.

#include <cstddef>
#include <string>
#include <vector>

#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The cell a point picks: its solids, and the tools that hold the point
// (indices into the tools, in increasing order).
struct BoundaryCell {
  ShapePtr shape;
  std::vector<std::size_t> holders;
};

// For each point, the cell it picks among the tools: all of the space
// inside every tool that holds the point and outside every other tool, as
// one shape of one or more solids. The tools' faces divide space once
// (OCCT's general fuse, BOPAlgo_CellsBuilder), so cells that meet share
// their faces; the pieces of one face that a cell has side by side are
// merged again. Each face keeps the names of the tool faces it is a piece
// of (several where faces of tools lie on each other), pieces of one face
// numbered "#k" (naming.hpp). Two points in one cell give the same shape.
// Throws std::runtime_error naming the point when no tool holds it, when it
// lies on a tool's boundary, or when its cell comes out empty, and when a
// tool has no solid. The tools are not changed.
std::vector<BoundaryCell> boundary_cells(const std::vector<const Shape*>& tools,
                                         const std::vector<gp_Pnt>& points);

// --- Surface bodies and planes as tools (mitcad#2149)

// A closed region of boundary_regions: its solid and a point inside it.
struct BoundaryRegion {
  ShapePtr shape;
  gp_Pnt point;
};

// Where a point asked about lies.
struct BoundaryPointPlace {
  enum class Kind { Region, OnFace, Open };
  Kind kind = Kind::Open;
  std::size_t region = 0; // Region: the index of the region
  // Open: points along the free edges of the tools that the open region
  // reaches, where the surfaces end and it leaks out; may be empty.
  std::vector<gp_Pnt> gap;
};

struct BoundaryRegions {
  std::vector<BoundaryRegion> regions;
  std::vector<BoundaryPointPlace> places; // one per point, in their order
};

// The closed regions that the faces of the tools (solids and surface
// bodies, untrimmed and overlapping) and the planes bound together: all the
// faces divide space once (OCCT's BOPAlgo_MakerVolume), and every part of
// space they close off all round is a region; faces that close nothing off
// are left out. A plane is cut to a square that reaches past the box of
// the tools and the points (with planes alone: of the points where three
// planes cross); its pieces are named `plane_names[i]`. The tools' pieces
// keep the names of the tool faces they are pieces of, pieces of one face
// side by side merged again, as boundary_cells' cells. The regions come in
// the order of their points (x, then y, then z). Each point is placed in
// the region holding it, on a face, or in an open region, with points
// along the free edges of the tools where that region leaks out. Throws
// std::runtime_error when a tool has no faces or the faces cannot be
// divided. The tools are not changed.
BoundaryRegions boundary_regions(const std::vector<const Shape*>& tools, const std::vector<gp_Pln>& planes,
                                 const std::vector<std::string>& plane_names, const std::vector<gp_Pnt>& points);

} // namespace mitcad::geometry
