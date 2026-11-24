// SPDX-License-Identifier: MIT
#pragma once

// Lofts with end conditions or rails (see loft.hpp). The sections are made
// compatible edge for edge; each side face is a surface whose rows
// interpolate the sections' poles together with the derivatives the end
// conditions ask for, then bent through the rails; the faces are sewn with
// the caps into a solid. Internal to the geometry library.

#include <optional>
#include <utility>
#include <vector>

#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>

#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/shape.hpp"
#include "spine.hpp"

namespace mitcad::geometry::detail {

// A section to skin: its outer wire, or a vertex; or an open wire, a
// chain of curves (mitcad#2345: no corners at its ends, no rails, no caps).
struct SkinSection {
  TopoDS_Shape shape;
  bool point = false;
  std::optional<gp_Dir> normal; // the plane of a sketch region or of a chain
  ShapePtr body;                // a face of a body: the body and the face
  TopoDS_Face face;
  // Tangent Edges: Merge (mitcad#2162): each joined edge of `shape` with
  // the section's own edges it joins (a face's: tangent and smooth ends
  // follow the faces next to those).
  std::vector<std::pair<TopoDS_Edge, std::vector<TopoDS_Edge>>> sources;
  // A Perpendicular end's face (mitcad#2345), found on its body; null: the
  // section's own face, or its plane (`normal`).
  TopoDS_Face perpendicular;
};

// The solid through the sections (two or more, points only first or last),
// side faces named after the edges of `named` (one of the sections), caps
// `first` and `last` (empty for none). Names are not numbered yet. `closed`:
// from the last section back to the first, periodic and tangent-continuous,
// without caps; `solid` false: the side faces only, a shell. Without end
// conditions and rails the surfaces are the piecewise cubic Hermite ones
// (HermiteInterpolation).
//
// `seams_only` (mitcad#2141, mitcad#2143): two sections with free ends and
// no rails, the faces ruled across matched edges as OCCT's through-sections
// rules them, but tangent all along every line between matched vertices
// through which both sections run on smoothly, the faces of a chain joined
// so pieces of one surface continuous in its first derivative (periodic
// round the whole section; match_seams in loft_skin.cpp); null when no
// such line needs it, or for other lofts.
//
// `matched`: every section that is not a point is one edge starting at the
// point matching its neighbour's, running the same way round (merged
// tangent edges, mitcad#2162): lofted as they are, without OCCT's matching
// of the sections.
//
// A side that is one surface all round the sections (one closed edge per
// section, or a chain of `seams_only` round the whole section) is made
// periodic where it closes smoothly, as the kernel's offset takes a
// closed surface (mitcad#2141, mitcad#2271).
ShapePtr skinned_loft(const std::vector<SkinSection>& sections, const LoftEnd& start, const LoftEnd& end,
                      const std::vector<Path>& rails, const NamedWire& named, const NameList& first,
                      const NameList& last, bool closed = false, bool solid = true, bool seams_only = false,
                      bool matched = false);

// (mitcad#2271) Each face of the loft (built just now, owned by the
// caller) on a B-spline surface that closes on itself round the sections,
// smoothly, put on that surface made periodic (as skinned_loft makes its
// sides), its edges' curves on it carried over: the same points within
// 1e-6 mm, the same faces, edges and names. OCCT's through-sections closes
// such a side as a clamped surface, which the kernel's offset does not take
// outward. True when a face changed.
bool periodic_closed_sides(const TopoDS_Shape& loft);

} // namespace mitcad::geometry::detail
