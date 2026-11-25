// SPDX-License-Identifier: MIT
#pragma once

// Edges whose two faces come tangent somewhere inside them, the edge
// turning there from convex to concave or back (mitcad#2300): the rim
// between a loft's side and a cylinder pulled from its end face, where the
// side flares out beyond the cylinder in one direction and draws in within
// it in another. A rolling ball of one radius cannot run across such a
// point: on the convex side it lies inside the material's angle, on the
// concave side outside it, and at the point itself the rounding has no
// width. The rounding of such an edge vanishes there, as it does at a
// vertex where a convex edge runs on into a concave one (face_select.hpp,
// follow_tangent_chains).
//
// OCCT's fillet takes an edge as convex or concave all along (its contour's
// one type of concavity, ChFiDS_Spine::SetTypeOfConcavity), so its walk
// along such an edge stops where the rounding vanishes: the ball's contact
// runs off its face over the edge itself, and the walk rolling on that edge
// fails ("stops part of the way where it rolls on an edge of a neighbouring
// face"). At a vertex where the faces come tangent its contours stop, and
// the roundings that end there vanish into it, each in a point (port
// patches 0771, 1402, 2142 and PerformFilletOnVertex). So the edge is split
// at the points where its faces come tangent, each piece convex or concave
// all along, and rounded on that body. Internal to the geometry library.

#include <map>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>

#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// The parameters (of the edge's own curve, inside its range, increasing)
// where the outward normals of the edge's faces `a` and `b` pass through
// parallel, the edge turning from convex to concave or back there; more
// than 1e-3 mm from the edge's ends. Empty where the edge keeps one
// convexity, or its faces' normals cannot be worked out along it.
std::vector<double> vanishing_parameters(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b);

// A body with edges split where their roundings vanish, and the fillet's
// sets for it.
struct VanishingSplit {
  ShapePtr body;
  // The sets with their edges named on `body`: every piece of a selected
  // edge, and of an edge its tangent chain takes on (the chain followed
  // across vertices where the edges meet end to end and are both convex
  // or both concave), by its own name; no faces.
  std::vector<FilletSet> sets;
  // The edges of `body` that are pieces of an edge of the input: the input
  // edge's index and the piece's place along it (0 first, along the edge's
  // curve).
  std::map<int, std::pair<int, int>> pieces;
};

// `input` with the edges the sets round (constant roundings only) and
// those their tangent chains take on (the kernel's contours of the
// selected edges, and MitCad's chain) split at their vanishing_parameters.
// None where no such edge has one, a set is not a constant rounding, or
// the split body is not valid; `why`, when given, then says which.
std::optional<VanishingSplit> split_at_vanishing_points(const Shape& input, const std::vector<FilletSet>& sets,
                                                       std::string* why = nullptr);

// The renames from the split body's edge and vertex names to the input's:
// a piece's name to its edge's (with its place along it), an edge or
// vertex the split left as it was to its name in the input where that
// differs.
struct UnsplitNames {
  struct Rename {
    std::string to;
    int piece = -1; // a piece's place along its edge; -1 for none
  };
  std::map<std::string, Rename> renames;
};
UnsplitNames unsplit_names(const Shape& input, const VanishingSplit& split);

// The text with the split body's names put back to the input's (a piece's
// name to its edge's): for the error of a fillet built on the split body.
std::string with_unsplit_names(const std::string& text, const UnsplitNames& names);

// The fillet's result on the split body with its new faces (those named
// `<feature>:...`) named after the input's edges and vertices: the
// rounding along a piece of an edge `<feature>:fillet(<edge>)#k`, k the
// piece's place along the edge (the pieces of one face as the face's split
// pieces, so that the edge's name finds the rounding of all of them).
ShapePtr with_unsplit_names(const ShapePtr& result, const UnsplitNames& names, const std::string& feature);

} // namespace mitcad::geometry::detail
