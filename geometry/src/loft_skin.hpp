// SPDX-License-Identifier: MIT
#pragma once

// Lofts with end conditions or rails (see loft.hpp). The sections are made
// compatible edge for edge; each side face is a surface whose rows
// interpolate the sections' poles together with the derivatives the end
// conditions ask for, then bent through the rails; the faces are sewn with
// the caps into a solid. Internal to the geometry library.

#include <optional>
#include <vector>

#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>

#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/shape.hpp"
#include "spine.hpp"

namespace mitcad::geometry::detail {

// A section to skin: its outer wire, or a vertex.
struct SkinSection {
  TopoDS_Shape shape;
  bool point = false;
  std::optional<gp_Dir> normal; // the plane of a sketch region
  ShapePtr body;                // a face of a body: the body and the face
  TopoDS_Face face;
};

// The solid through the sections (two or more, points only first or last),
// side faces named after the edges of `named` (one of the sections), caps
// `first` and `last` (empty for none). Names are not numbered yet.
ShapePtr skinned_loft(const std::vector<SkinSection>& sections, const LoftEnd& start, const LoftEnd& end,
                      const std::vector<Path>& rails, const NamedWire& named, const NameList& first,
                      const NameList& last);

} // namespace mitcad::geometry::detail
