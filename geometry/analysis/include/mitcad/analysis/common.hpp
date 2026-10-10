// SPDX-License-Identifier: MIT
#pragma once

// Common types of Mitcad's geometry analysis library
// (mitcad_geometry_analysis): physical properties, measurement,
// interference, sections and shape comparison on OCCT shapes.
//
// All lengths are millimetres and angles radians. Functions throw
// mitcad::analysis::Error for input they cannot analyse (for example a
// curved face where a planar one is needed) or when OCCT fails.

#include <stdexcept>

#include <GProp_GProps.hxx>
#include <TopoDS_Shape.hxx>

namespace mitcad::analysis {

class Error : public std::runtime_error {
public:
  using std::runtime_error::runtime_error;
};

struct Vec3 {
  double x = 0.0;
  double y = 0.0;
  double z = 0.0;
};

// Axis-aligned bounding box.
struct Bounds {
  Vec3 min;
  Vec3 max;
  bool empty = true;

  Vec3 size() const { return {max.x - min.x, max.y - min.y, max.z - min.z}; }
};

// Exact bounding box from the geometry (from the triangulation for mesh
// bodies), without the shape tolerances.
Bounds bounds(const TopoDS_Shape& shape);

// The solids of a shape, each oriented so that its volume is positive
// (imported or reversed shapes may be inside out).
TopoDS_Shape oriented_solids(const TopoDS_Shape& shape);

// Volume and surface integrals of a shape (mass, centre, inertia), for
// every volume and area Mitcad reports, so that they agree. Each face is
// integrated span by span (mitcad#140): along its boundary curves in the
// parameter space and across the surface, split at the surface's and the
// curves' knots, where the curves cross the surface's knot lines and every
// eighth of a turn of an angle parameter, with fixed Gauss rules exact for
// polynomial pieces (more points for rational and offset geometry, and
// rational spans in quarters). OCCT's own rules miss: the fixed one takes
// at most 61 points across a face and along a boundary curve (a hole
// bounded by a cubic of 132 spans took 141 mm² off its face, mitcad#139; a
// prism over a closed cubic of 200 spans came out 1.4 % small), and the
// adaptive one never refines across a face and drops a span of a boundary
// curve of more than 1953 (a band between helices stored as cubics of 2574
// spans, 0.3 % off its area). Faces reaching to infinity keep OCCT's fixed
// rule; faces without a surface (meshes) are integrated over their
// triangles. The tests' faces of every kind come within 1e-9 (relative) of
// their exact values.
//
// `only_closed`: closed shells only; `skip_shared`: a face shared by two
// shells once.
GProp_GProps volume_properties(const TopoDS_Shape& shape, bool only_closed = false,
                               bool skip_shared = false);
GProp_GProps surface_properties(const TopoDS_Shape& shape, bool skip_shared = false);

} // namespace mitcad::analysis
