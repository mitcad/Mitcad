// SPDX-License-Identifier: MIT
#pragma once

// Building blocks of patterns, mirrors and Combine: the union of shapes and
// the tool solid that faces of a body bound.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The union of the shapes, their face names carried through: one shape of
// one or more solids (shapes that do not touch stay separate solids).
// Coplanar faces that meet merge, as after a Join.
ShapePtr unite(const std::vector<const Shape*>& shapes);

// The closed solid the named faces of a body bound together with planar
// caps across the openings where they meet the rest of the body; the faces
// keep their names, the caps have none. `material` tells whether the solid
// is material of the body (a boss, to join) or empty space (a pocket or a
// hole, to cut). Throws when an opening does not lie in a plane.
struct FaceTool {
  ShapePtr tool;
  bool material = true;
};

FaceTool face_tool(const Shape& body, const std::vector<std::string>& faces);

} // namespace mitcad::geometry
