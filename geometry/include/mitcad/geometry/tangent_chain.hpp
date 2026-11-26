// SPDX-License-Identifier: MIT
#pragma once

// The tangent chains of a body (mitcad#290): which edges a fillet or
// chamfer with a tangent chain rounds together, and which faces a face
// operation with a tangent chain (shell, draft, replace face, offset face) takes
// together, so that the application's hover can show the whole chain
// before the pick.
//
// The edge chains follow Mitcad's own chain of the dressups (dressup.hpp,
// mitcad#224): an edge continues into every edge that meets it end to end
// in the same direction (within 0.01 rad) and lies between faces at an
// angle, and so on from those. OCCT's fillet contour stops where its edges
// turn (mitcad#241), so it takes no edge past where this chain stops. The
// face chains are the faces connected through smooth edges, as the face
// operations grow their selections.
//
// A chain is grown once per body: every edge (face) of the body belongs to
// at most one chain, and an edge in none stands alone.

#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The chains of two or more edges (indices of `body.edge(i)`, increasing),
// ordered by their first edge. An edge between tangent faces (no dressup
// rounds it) is in none.
std::vector<std::vector<int>> edge_tangent_chains(const Shape& body);

// The chains of two or more faces (indices of `body.face(i)`, increasing),
// ordered by their first face.
std::vector<std::vector<int>> face_tangent_chains(const Shape& body);

} // namespace mitcad::geometry
