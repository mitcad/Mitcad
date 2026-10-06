// SPDX-License-Identifier: MIT
#pragma once

// Splitting bodies and faces with a tool (Split Body and Split Face).

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/tool.hpp"

namespace mitcad::geometry {

// Splits the body with a plane, a face (of any body) or another body into
// solids. With `extend`, a face tool is extended along its surface so that
// it cuts through the whole body (planes always are). The pieces come in
// order of their centre along the plane's normal for a planar tool, else in
// geometric order; fails when the tool does not divide the body. The cut
// faces are named <feature>:split, after a face tool's face
// <feature>:split(<tool face>); the body's faces keep their names.
std::vector<ShapePtr> split_body(const std::string& feature, const Shape& body, const Tool& tool,
                                 bool extend = true);

// Splits the named faces along their intersection with the tool (a plane, a
// face, a body, or sketch curves swept along a direction). The body's shape
// and volume do not change; the pieces of each face keep its name with
// "#k". Fails when a face is not split.
ShapePtr split_faces(const std::string& feature, const Shape& body,
                     const std::vector<std::string>& faces, const Tool& tool, bool extend = true);

} // namespace mitcad::geometry
