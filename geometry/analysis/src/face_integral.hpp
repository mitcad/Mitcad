// SPDX-License-Identifier: MIT
#pragma once

// Internal: the integrals of a face with a surface, span by span.

#include <optional>

#include <GProp_GProps.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>

namespace mitcad::analysis::detail {

enum class Integral { Volume, Surface };

// The volume integral (the solid the face bounds towards `at`, from its
// outward normal) or the surface integral of a face, about `at`: mass,
// centre and matrix of inertia, as OCCT's BRepGProp_Vinert and
// BRepGProp_Sinert give them. Empty for a face whose surface reaches to
// infinity, which this integration does not handle.
std::optional<GProp_GProps> integrate_face(const TopoDS_Face& face, const gp_Pnt& at, Integral integral);

} // namespace mitcad::analysis::detail
