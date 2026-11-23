// SPDX-License-Identifier: MIT
#pragma once

// Fillet's Full Round (mitcad#2279): the radius that rounds a face away
// between its two edges, worked out on the body on every build.
//
// The set's edges are the two edges (or tangent chains) of one face, the
// middle face; each chain's other face is a side face. Two equal roundings
// of the middle face's two edges meet exactly on it when one circle in the
// section across the edges touches both side faces and the middle face:
// the radius follows from those three faces (with an angled side face it is
// not half the face's width). It is found at stations along every edge of
// both chains that borders the middle face: in the plane across the edge
// there, the circle whose distance to each face's tangent plane is its
// radius, on the side the rounding's ball rolls (inside the material at
// convex edges, outside it at concave ones); each face's tangent plane is
// taken again where the circle touches the face until the circle no longer
// moves (at once for planes; for curved faces a few steps). The radius must
// be the same at every station within kFullRoundTolerance (checked first,
// so that a tapered face is refused as such), and the circle must touch the
// middle face inside it, between its edges, at the middle of each edge (a
// section near an edge's end may pass an oblique end of the face).
//
// Refused in words (std::invalid_argument, placed_error: the edges, the
// middle and side faces painted) where a face is named instead of edges,
// the edges are not two chains of one face (no face shared, the same two
// faces, chains that meet at a corner or more than two), the two edges are
// not both convex or both concave, no such circle touches the middle face
// between its edges, or the radius would change along the edges (faces that
// taper or twist). A full round whose radius varies is not built.

#include <string>
#include <vector>

#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// How much the radius may change along the edges, mm: the modelling
// tolerance the rounding's contacts on the side faces keep.
inline constexpr double kFullRoundTolerance = 1.0e-6;

// A full round set worked out on the body.
struct FullRound {
  double radius = 0.0;
  // The face it rounds away.
  int middle = -1;
  // The edges of both chains that border the middle face (with the edges
  // the tangent chains add where the set follows them).
  std::vector<int> edges;
};

// The full round of the set's edges (FilletSize::FullRound) on the body;
// throws as above.
FullRound full_round(const Shape& body, const FilletSet& set);

// The result of a fillet with full rounds (worked out on `body`, the
// fillet's input) with each middle face's names put onto the faces of the
// result named after the round's edges (`<feature>:fillet(<edge>)`), so
// that what referred to the middle face finds the rounding that took its
// place. A middle face the result still holds keeps its names alone. Null
// stays null.
ShapePtr with_full_round_names(const ShapePtr& result, const Shape& body, const std::string& feature,
                               const std::vector<FullRound>& rounds);

} // namespace mitcad::geometry::detail
