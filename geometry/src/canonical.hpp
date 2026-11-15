// SPDX-License-Identifier: MIT
#pragma once

// The analytic surfaces and curves of a dressup's B-spline faces and edges
// (mitcad#328). Where OCCT's fillet walks a rounding (a ball rolling on a
// narrow face's far edge, the port's edge overflow, or a rounding along a
// walked one), it builds the rounding as a B-spline surface and its edges
// as B-spline curves even where they are exactly a cylinder, lines and
// circles. The bodies the files store have the cylinder there, and a
// rounding later run along those faces fails in OCCT's walk on the
// B-splines and builds on the analytic ones.

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// `result` with the faces it has and `input` lacks whose B-spline surfaces
// are a plane, cylinder, cone, sphere or torus within OCCT's confusion
// tolerance (1e-7) given that surface, and the B-spline edges of those
// faces that are lines or circles within it given that curve; the faces
// keep their names, the shape its notes. Null when nothing converts, a face
// of the result is closed on a seam, or a converted face does not check
// valid (the result then stays as OCCT built it). `exact` widens that
// tolerance for shapes OCCT approximated more loosely (a helix's sweep,
// path_sweep.cpp). With `seams_away` a new cylinder, cone, sphere or torus
// has its seam opposite the face's middle, through none of the face
// (mitcad#1849: the path past narrow faces bounds its regions by the
// surface with its own seam).
ShapePtr canonical_dressup(const Shape& input, const Shape& result, double exact = 1e-7, bool seams_away = false);

} // namespace mitcad::geometry::detail
