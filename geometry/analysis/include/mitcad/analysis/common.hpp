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
// every volume and area Mitcad reports, so that they agree. OCCT's default
// fixed Gauss points are accurate on planes, quadrics, tori and polynomial
// patches but off by about 1 % on rational ones (a cylinder scaled
// non-uniformly); those faces, and surfaces of revolution, extrusion and
// offset, are integrated adaptively to kIntegrationTolerance (relative,
// per face). Faces without a surface (meshes) are integrated over their
// triangles.
inline constexpr double kIntegrationTolerance = 1e-9;

// `only_closed`: closed shells only; `skip_shared`: a face shared by two
// shells once.
GProp_GProps volume_properties(const TopoDS_Shape& shape, bool only_closed = false,
                               bool skip_shared = false);
GProp_GProps surface_properties(const TopoDS_Shape& shape, bool skip_shared = false);

} // namespace mitcad::analysis
