// SPDX-License-Identifier: MIT
#pragma once

// Booleans of a large body with a small tool that work only on the body's
// faces near the tool (modelled threads). Internal to the geometry library.
//
// OCCT's booleans rebuild the whole shell of every solid whose faces they
// split, and its checker goes through every face: on a body of thousands
// of faces both take seconds to minutes (quadratic in the faces), however
// small the change. Here the faces whose bounding boxes do not reach the
// tool's stay as they are: the general fuse splits only the near faces and
// the tool's, the pieces are classified by the faces around them, and the
// result's shell is the far faces with the kept pieces; the checker sees
// only the faces that changed. When that cannot be done (several solids or
// shells, a tool of several solids, a result that falls apart, a piece
// that cannot be classified), the ordinary boolean runs instead.

#include <string>

#include <TopoDS_Shape.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

enum class LocalOperation { Cut, Fuse };

struct LocalResult {
  // The result with the body's face names carried and the tool's faces
  // named `tool_name`; split names are not numbered yet (FaceNamer::finish).
  ShapePtr shape;
  // Whether only the near faces took part.
  bool local = false;
};

// `body` (one solid) with `tool` (solids) cut out of it or fused to it.
LocalResult local_boolean(const Shape& body, const TopoDS_Shape& tool, LocalOperation operation,
                          const std::string& tool_name);

// Whether the faces are valid (OCCT's checker on these faces only) and the
// shell around them closed and consistently oriented: every edge of the
// changed faces is shared by two faces of the result that run along it in
// opposite directions. Throws like require_valid otherwise.
void require_valid_near(const TopoDS_Shape& result, const TopoDS_Shape& changed, const char* operation);

} // namespace mitcad::geometry::detail
