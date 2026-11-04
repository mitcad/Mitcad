// SPDX-License-Identifier: MIT
#pragma once

// A shell built as the body less a cavity (mitcad#1233), where the
// geometry kernel's thick solid fails: parts of the body no thicker than
// the wall stay solid. Internal to the geometry library; faceops.cpp's
// shell falls back on it.

#include <set>
#include <string>

#include <GeomAbs_JoinType.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// `from` (the body, or its outside offset for a closed shell with an
// outside wall) less the cavity of `body` hollowed inside by `thickness`
// with the faces `removed` (indices into `body`) open. The cavity is the
// body's inward offset with the removed faces moved out past the body, so
// that it opens through them; it leaves out:
//   - a plate between two opposite planar faces no further apart than twice
//     the wall, cut off the body before the offset (its faces stay solid);
//   - a curved or tapered part thinner than twice the wall (a kept face
//     near enough to another across the body under at least half of it, and
//     the faces it turns away from convexly), taken away before the offset
//     with the faces round it extended over its root (mitcad#1237); where
//     no face round it can be extended so (a fin whose ends continue the
//     body's sides) and the offset fails, cut off at its root by the
//     surface the faces it stands on lie on (mitcad#1240);
//   - the part under a face no further from a removed face than the wall
//     (its offset lies past the removed face, outside the body);
//   - when the offset fails on them, convex roundings tighter than the wall
//     (and the curved faces round them that go with them): the inside of
//     the wall runs past them.
// Where the cavity comes nearer than the wall to a face it leaves out, the
// faces its nearest part is offset from go deeper by the difference.
// The faces of `from` keep their names; the cavity's are
// <feature>:offset(<face>) and what is left of a removed face
// <feature>:offset_cap(<face>). The result's notes name the faces the wall
// does not fit under and the faces it runs past.
//
// Returns null when the geometry kernel cannot build the cavity; throws
// std::runtime_error when the wall fits under no face.
ShapePtr shell_by_cavity(const std::string& feature, const Shape& body, const std::set<int>& removed,
                         double thickness, GeomAbs_JoinType join, const Shape& from);

// Notes on the result of a shell of the body (inside, by `thickness`, the
// faces `removed` open) for the body's named faces that have no inside of
// the wall <feature>:offset(<face>) in it: where the inside of the wall is
// further than twice the wall from some of a face, the wall does not fit
// under it and the body is left solid there; else the wall runs past it (a
// rounding or a strip too small for it to follow). Where the inside of the
// wall is nearer than the wall to some of those faces, a note says how thin
// the wall gets beside them.
void note_missing_walls(const std::string& feature, const Shape& body, const std::set<int>& removed,
                        double thickness, Shape& result);

} // namespace mitcad::geometry::detail
