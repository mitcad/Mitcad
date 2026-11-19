// SPDX-License-Identifier: MIT
#pragma once

// Pieces of a face a fillet or chamfer takes away, left in its result
// between faces it made (mitcad#1858): closed by a patch at each place
// instead of the geometry kernel's defeaturing, which extends the faces
// round a piece to meet and, beside a blend's faces (each tangent to the
// faces it rounds and to the next blend face along the chain), meets
// surfaces tangent to each other: it ran for minutes and failed. Internal
// to the geometry library.

#include <string>
#include <vector>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// `result` (a fillet or chamfer of `input`, its faces named
// <feature>:<role>(<edge>) and <feature>:corner(<vertex>)) with the faces
// `pieces` (indices in `result`) closed: the pieces that share edges are one
// place; at each, the dressup's faces beside it (`made`, by face of
// `result`) are cut across their edges a setback past it (the place's size
// and half the dressup's reach `contact`; on along the input's next tangent
// edge where a face runs on past the end of the edge it is named after),
// as a setback corner cuts them (corner.hpp), or taken whole where they end before that and are small
// (a corner face; under 4 contact^2), and the pieces with the parts of those faces
// next to them are replaced by one patch (OCCT's plate surface,
// BRepOffsetAPI_MakeFilling) through the place's boundary, tangent to the
// faces outside it along the boundary where they should run on into it:
// the dressup's faces, faces the place already meets smoothly (a
// rounding's contacts), and for a fillet every face along the pieces' own
// edges (a piece of the chain's edge left sharp there); through the edges
// elsewhere (a chamfer's bevels meet the faces at an angle). Sewn into the
// solid. Each patch is named
// <feature>:corner(<vertex>) after the input's vertex that most of its
// dressup faces' edges share (where they meet), the nearest of those;
// every other face keeps its
// names (the faces cut keep theirs on the part left). Null where a place's
// patch cannot be built, does not meet the faces round it (1e-3 mm, 0.1
// radians), or the result is not one valid closed solid whose volume is
// within `contact` times the places' area of `result`'s.
ShapePtr patch_remnants(const Shape& result, const Shape& input, const std::vector<int>& pieces,
                        const std::vector<char>& made, const std::string& feature, const char* role,
                        double contact);

} // namespace mitcad::geometry::detail
