// SPDX-License-Identifier: MIT
#pragma once

// Boolean operations of a tool with the participant bodies of a feature:
// Join, Cut and Intersect.

#include <cstddef>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

enum class BooleanOp { Join, Cut, Intersect };

// A solid of the result and the targets (indices into the operation's
// targets) whose material it contains; empty for a piece made of the tool
// alone.
struct BooleanPiece {
  ShapePtr shape;
  std::vector<std::size_t> sources;
};

// `touched[i]`: target i takes part. An untouched target is unchanged and
// has no piece; the caller keeps it as it was. A touched target without a
// piece was removed (a cut through all of it). An Intersect touches a
// target lying wholly inside the tool: its one piece is the target, with its
// names (mitcad#1534).
struct BooleanResult {
  std::vector<BooleanPiece> pieces;
  std::vector<bool> touched;
};

// Join fuses the tool with every target it touches into one or more solids;
// a tool solid that touches no target is a piece of its own. Cut removes the
// tool from each target it overlaps and Intersect keeps the common part; a
// target may split into several pieces. Pieces are in geometric order. Face
// names of the targets and the tool carry through (see naming.hpp).
//
// `within` (mm, 0 for none) widens a Join where it fails exactly
// (mitcad#298), for an importer whose bodies match a file's only within a
// geometric tolerance: a tool that touches no target still joins those
// within `within` of it, and a join whose result is invalid is retried with
// a fuzzy value of up to `within`. Each takes the smallest fuzzy value that
// works; a join that works exactly stays exact. It widens a Cut alike
// (mitcad#235): a cut of a target that fails exactly (an error, an invalid
// result, faces its result's solids could not be built with, or pieces of
// the tool left inside the body) is retried with the smallest fuzzy value
// up to `within` that gives a valid result of all its faces.
BooleanResult boolean(BooleanOp op, const std::vector<const Shape*>& targets, const Shape& tool,
                      double within = 0.0);

} // namespace mitcad::geometry
