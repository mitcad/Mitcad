// SPDX-License-Identifier: MIT
#pragma once

// Replace face for faces and targets of any surface type (P5): the region
// between the replaced faces and the target, closed by the neighbouring
// faces continued along their surfaces, is added to or removed from the
// body. Internal to the geometry library; faceops.cpp keeps its own path
// for planar faces and planar targets.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/tool.hpp"

namespace mitcad::geometry::detail {

// Replaces the faces (indices into the body) with the target (a plane, a
// face of another body or another body's faces) and names the new face
// `name` (pieces "#k"); the neighbours keep their names.
ShapePtr replace_by_slab(const Shape& body, const std::vector<int>& faces, const Tool& target,
                         const std::string& name);

} // namespace mitcad::geometry::detail
