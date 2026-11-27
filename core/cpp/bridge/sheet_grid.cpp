// SPDX-License-Identifier: MIT
#include "bridge/sheet_grid.hpp"

#include <array>
#include <string>
#include <utility>
#include <vector>

#include "mitcad/geometry/sheet_face_grid.hpp"
#include "mitcad_bridge/kernel/sheet_grid.h"

namespace mitcad::bridge {
namespace {

void push_points(rust::Vec<double>& out, const std::vector<gp_Pnt>& points) {
  out.reserve(points.size() * 3);
  for (const gp_Pnt& p : points) {
    out.push_back(p.X());
    out.push_back(p.Y());
    out.push_back(p.Z());
  }
}

void push_vectors(rust::Vec<double>& out, const std::vector<gp_Vec>& vectors) {
  out.reserve(vectors.size() * 3);
  for (const gp_Vec& n : vectors) {
    out.push_back(n.X());
    out.push_back(n.Y());
    out.push_back(n.Z());
  }
}

SheetGridEdge converted(const geometry::FaceGridEdge& edge) {
  SheetGridEdge result;
  result.cells.reserve(edge.cells.size() * 2);
  for (const std::array<double, 2>& c : edge.cells) {
    result.cells.push_back(c[0]);
    result.cells.push_back(c[1]);
  }
  push_points(result.points, edge.points);
  push_vectors(result.normals, edge.normals);
  return result;
}

SheetGrid converted(const geometry::FaceGrid& grid) {
  SheetGrid result;
  push_points(result.points, grid.points);
  push_vectors(result.normals, grid.normals);
  result.area = grid.area;
  for (const geometry::FaceGridWire& wire : grid.wires) {
    SheetGridWire sampled;
    sampled.outer = wire.outer;
    for (const geometry::FaceGridEdge& edge : wire.edges) sampled.edges.push_back(converted(edge));
    result.wires.push_back(std::move(sampled));
  }
  return result;
}

} // namespace

rust::Vec<SheetGrid> sheet_face_grid(const geometry::Shape& shape, rust::Str face, std::size_t columns,
                                     std::size_t rows, std::size_t edge_steps) {
  rust::Vec<SheetGrid> pieces;
  for (const geometry::FaceGrid& grid :
       geometry::sheet_face_grid(shape, std::string(face), columns, rows, edge_steps)) {
    pieces.push_back(converted(grid));
  }
  return pieces;
}

} // namespace mitcad::bridge
