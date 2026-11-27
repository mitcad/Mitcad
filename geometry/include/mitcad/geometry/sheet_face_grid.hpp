// SPDX-License-Identifier: MIT
#pragma once

// A face sampled over its surface's parameters (mitcad#2442): the points,
// normals and area an approximate development by triangulation lays out
// and measures (core/model/src/sheet/folded/triangulated.rs), and its
// boundary, the outline and the holes laid out with it (mitcad#2558).

#include <array>
#include <cstddef>
#include <string>
#include <vector>

#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// An edge of a face's boundary at equal steps of its parameter, in the
// direction its wire runs.
struct FaceGridEdge {
  // Each point's place in the face's grid: x = i at column i, y = j at
  // row j, fractions between them.
  std::vector<std::array<double, 2>> cells;
  std::vector<gp_Pnt> points;
  // Unit normals out of the body, as FaceGrid::normals.
  std::vector<gp_Vec> normals;
};

// A wire of a face's boundary: its edges in the order the wire runs, edges
// of no length (a surface's collapsed side) left out.
struct FaceGridWire {
  // The face's outer wire; else a hole.
  bool outer = false;
  std::vector<FaceGridEdge> edges;
};

struct FaceGrid {
  // Row by row: point (i, j) at j * (columns + 1) + i, column i at the
  // i-th of `columns` equal steps of the first parameter over the face's
  // bounds, row j likewise of the second.
  std::vector<gp_Pnt> points;
  // Unit normals out of the body (the face's orientation); where the
  // surface has none (an apex, a collapsed edge), the normal a little way
  // towards the middle of the face.
  std::vector<gp_Vec> normals;
  // The face's own area (mm^2).
  double area = 0;
  // The face's boundary when asked for (edge_steps above 0), else empty.
  std::vector<FaceGridWire> wires;
};

// Every piece of the face named `face` (a face the kernel split, as a loft
// splits a side where the other section has a vertex, has several) sampled
// at (columns + 1) x (rows + 1) points, in the shape's face order, with its
// boundary's edges at `edge_steps` steps each when that is above 0. A name
// of no face, and no column or row, are errors.
std::vector<FaceGrid> sheet_face_grid(const Shape& shape, const std::string& face, std::size_t columns,
                                      std::size_t rows, std::size_t edge_steps = 0);

} // namespace mitcad::geometry
