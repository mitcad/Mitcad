// SPDX-License-Identifier: MIT
#pragma once

// Helpers of the emboss by projection (emboss_projection.cpp) that its
// tests reach. Internal to the geometry library.

#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

namespace mitcad::geometry::detail {

// A point inside the face, off its boundary, and the face's unit normal
// there (its orientation applied): a grid over the face's parameters, else
// (a sliver the grid misses) OCCT's search for a point in a face. False
// when neither finds one.
bool inner_point(const TopoDS_Face& face, gp_Pnt& point, gp_Vec& normal);

} // namespace mitcad::geometry::detail
