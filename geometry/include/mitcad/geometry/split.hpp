// SPDX-License-Identifier: MIT
#pragma once

// Splitting bodies and faces with a tool (Split Body and Split Face).

#include <string>
#include <vector>

#include <gp_Pnt.hxx>

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
// "#k". A region of the tool that crosses a face's seam (a cylinder's or a
// cone's) is one piece: the split's pieces on either side of the seam are
// joined again, unless a cut runs along it (mitcad#1853). Fails when a face
// is not split.
ShapePtr split_faces(const std::string& feature, const Shape& body,
                     const std::vector<std::string>& faces, const Tool& tool, bool extend = true);

// The faces split_body, split_faces and trim_surface cut with when their
// bodies lie in the box from `low` to `high`, made the same way: a plane
// as a face across the box, a face tool extended along its surface with
// `extend`, a body's faces or sketch curves swept through the box; clipped
// to the box. The preview of an extended tool (mitcad#712); the tool's
// shapes are left as they are. Fails when nothing of the tool lies in the
// box.
ShapePtr extended_tool(const Tool& tool, const gp_Pnt& low, const gp_Pnt& high, bool extend = true);

} // namespace mitcad::geometry
