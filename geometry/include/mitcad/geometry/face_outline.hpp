// SPDX-License-Identifier: MIT
#pragma once

// Outlines of faces seen along a direction (mitcad#1265): what Project of a
// curved face gives a sketch, the face as the sketch's normal sees it.
// Lengths are millimetres. The shapes are left as they are.

#include <string>
#include <vector>

#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry {

// The faces a face name resolves to seen along `direction`: their boundary
// edges as curves_of lists them (each edge once), except a seam edge that is
// not on the outline (where the face's normal is not at right angles to
// `direction` all along it, or the seam is seen end on, as a point), and
// then each face's silhouettes (face_silhouettes). Empty when nothing
// matches.
std::vector<ModelCurve> face_outline(const Shape& shape, const std::string& name, const gp_Dir& direction);

// The silhouettes of a face seen along `direction`: the curves on it along
// which its normal is at right angles to `direction`, clipped to the face.
// A cylinder or a cone has the generator lines where it turns away from the
// view (two of a whole one, as many of a part of one as its angle holds;
// none seen along its axis, or from inside a cone's angle), worked out
// exactly and clipped to the face's boundary; other curved faces have the
// outline curves of OCCT's hidden line removal (visible and hidden ones,
// each once); a plane, and a face without a surface, none. A silhouette
// seen end on (a line along `direction`) is left out.
std::vector<ModelCurve> face_silhouettes(const TopoDS_Face& face, const gp_Dir& direction);

} // namespace mitcad::geometry
