// SPDX-License-Identifier: MIT
#pragma once

// Planar sections of bodies and the clipped view of section analysis.

#include <cstddef>

#include "mitcad/analysis/common.hpp"

namespace mitcad::analysis {

// A plane through `origin` with normal `normal` (any length but zero).
struct Plane {
  Vec3 origin;
  Vec3 normal{0.0, 0.0, 1.0};
};

struct Section {
  // Compound of the section edges (the intersection with all faces).
  TopoDS_Shape curves;
  // Compound of the planar faces where the plane cuts the solids, with
  // holes; empty when the shape has no solids.
  TopoDS_Shape faces;
  std::size_t edge_count = 0;
  std::size_t face_count = 0;
  double length = 0.0; // of the curves, mm
  double area = 0.0;   // of the faces, mm^2
};

// Intersects a shape with a plane.
Section section(const TopoDS_Shape& shape, const Plane& plane);

// The shape with the half-space on the normal side of the plane removed,
// as section analysis shows it (the normal points towards the viewer).
TopoDS_Shape clip(const TopoDS_Shape& shape, const Plane& plane);

} // namespace mitcad::analysis
