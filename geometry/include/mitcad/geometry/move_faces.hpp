// SPDX-License-Identifier: MIT
#pragma once

// Faces of a body moved by a rigid motion (the .f3d import's moved faces,
// mitcad#343).

#include <string>
#include <vector>

#include <gp_Trsf.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// Moves the faces by the rigid motion `motion`: each keeps its surface,
// moved. The neighbouring faces stay on their surfaces and extend or
// shorten up to the moved ones: their common edges become the surfaces'
// intersections next to where the edges were, the vertices between them
// follow. Every face keeps its names; the body's topology stays as it is,
// and a motion that would have to change it (a face that would vanish or
// split, an edge whose faces no longer meet near it, an edge carried past
// the boundary of a face that stays) is an error. All faces of a boss
// moved along the face it stands on slide it, however far (mitcad#2543).
// The motion must be rigid (no scaling) and must move something.
//
// Planar faces in one plane translated along its normal (mitcad#2548) end
// on the level of another planar face of the body where they come within
// 1e-6 mm of it, and may cross the body's other faces: where the rebuild
// above cannot keep the topology (a raised block's end moved flush with
// the side of the base under it, or past it), the region the faces sweep
// is added to the body or removed from it and the faces around are
// trimmed where they meet. The moved faces keep their names (one that
// joins a coplanar face holds both faces' names), the other faces keep
// theirs, and a new face that continues none (an overhang's underside) is
// "<feature>:step(<face>)".
ShapePtr move_faces_rigidly(const std::string& feature, const Shape& body,
                            const std::vector<std::string>& faces, const gp_Trsf& motion);

} // namespace mitcad::geometry
