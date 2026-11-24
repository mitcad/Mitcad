// SPDX-License-Identifier: MIT
#pragma once

// Dressups of a straight edge between two planes that end on a flat face
// of a rounded body smaller than the dressup's section (mitcad#2229,
// GitHub mitcad/Mitcad#566). A body rounded all round keeps a flat face at
// the end of such an edge where its roundings leave one (a block rounded
// on its sides by half its width: the flat core of the rounding seen from
// the side); that face ends in a rounding of its own (the core's outline
// swept by the ball), tangent to it, within the section of a bevel or
// rounding of the edge as soon as the dressup reaches past the face. The
// geometry kernel ends a dressup on the face at the end of its edge, cut by
// that face's surface: where the face is smaller than the section, the
// bevel stands out of the body past the face's outline and the rounding
// beyond is left untrimmed under it, or the kernel fails. That is the
// kernel's to build (its end of a blend, ChFi3d); here only the case where
// no such dressup exists is found, to be refused in words before the
// kernel: its contact lies past both faces beside the edge.

#include <functional>
#include <optional>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// How far a dressup of a selected edge reaches along a face of it: the
// bevel's leg on that face, or for a rounding its radius (edge and face by
// index in the body).
using SectionReach = std::function<double(int edge, int face)>;

// A dressup wider than the body at a selected edge: the edge (index in the
// body) and the largest size that fits, as a share of the size asked (the
// leg or the radius times it touches both faces beside the edge).
struct WiderThanBody {
  int edge = -1;
  double fits = 0.0;
};

// Where every selected edge (`owner`: a set per edge, -1 for none) is a
// straight convex edge between two planes, ends at both vertices on one
// planar face that ends only in edges tangent to the faces beyond it (a
// flat face of a rounded body), the dressup's section (a bevel's triangle,
// `round` false, or the corner a rounding takes away, `round` true; `reach`
// its size) projected along the edge onto one of those faces' planes runs
// out of the face with no material of the body past the plane there, and
// the dressup's contact lies past both faces beside an edge (it touches
// neither): that edge and the largest size that fits. None elsewhere.
std::optional<WiderThanBody> wider_than_the_body(const Shape& input, const std::vector<int>& owner,
                                                 const SectionReach& reach, bool round);

} // namespace mitcad::geometry::detail
