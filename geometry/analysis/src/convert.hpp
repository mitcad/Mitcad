// SPDX-License-Identifier: MIT
#pragma once

// Internal conversions between the API types and OCCT.

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "mitcad/analysis/common.hpp"

namespace mitcad::analysis::detail {

inline Vec3 vec(const gp_XYZ& p) { return {p.X(), p.Y(), p.Z()}; }
inline Vec3 vec(const gp_Pnt& p) { return vec(p.XYZ()); }
inline Vec3 vec(const gp_Dir& d) { return vec(d.XYZ()); }
inline gp_Pnt pnt(const Vec3& v) { return gp_Pnt(v.x, v.y, v.z); }

// True when some face of the shape has no surface (a mesh body).
bool has_mesh_faces(const TopoDS_Shape& shape);

// True when the triangulations of the shape's faces form a closed surface
// (every triangle edge is shared by exactly two triangles).
bool closed_mesh(const TopoDS_Shape& shape);

} // namespace mitcad::analysis::detail
