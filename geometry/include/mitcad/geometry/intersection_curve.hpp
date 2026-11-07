// SPDX-License-Identifier: MIT
#pragma once

// Intersection curves (mitcad#1405): the 3D curve two sketches' curves make
// when each is swept along its own sketch's normal, where the two swept
// surfaces cross. Lengths are millimetres. The curves are left as they are.

#include <vector>

#include <gp_Dir.hxx>

#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace mitcad::geometry {

// Where the surface of the chain `first` (curves end to end in order, as a
// path's) swept along `first_direction` crosses the surface of the chain
// `second` swept along `second_direction`. Each chain is swept both ways
// from where it lies, far enough to pass through the other chain wherever
// it is (beyond the box of both). The crossing's pieces come as curves_of
// gives an edge's curve, each once and in no particular order; a piece ends
// where a face of either surface ends (where two curves of a chain meet).
// Empty when the surfaces do not cross. Throws for directions less than
// 1e-9 radians apart (parallel either way), for chains whose curves do not
// join, and when the section fails.
std::vector<ModelCurve> intersection_curves(const Path& first, const gp_Dir& first_direction, const Path& second,
                                            const gp_Dir& second_direction);

} // namespace mitcad::geometry
