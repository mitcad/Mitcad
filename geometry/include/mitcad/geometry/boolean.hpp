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
// piece was removed (a cut through all of it).
struct BooleanResult {
  std::vector<BooleanPiece> pieces;
  std::vector<bool> touched;
};

// Join fuses the tool with every target it touches into one or more solids;
// a tool solid that touches no target is a piece of its own. Cut removes the
// tool from each target it overlaps and Intersect keeps the common part; a
// target may split into several pieces. Pieces are in geometric order. Face
// names of the targets and the tool carry through (see naming.hpp).
BooleanResult boolean(BooleanOp op, const std::vector<const Shape*>& targets, const Shape& tool);

} // namespace mitcad::geometry
