// SPDX-License-Identifier: MIT
#pragma once

// Common types of MitCad's geometry analysis library
// (mitcad_geometry_analysis): physical properties, measurement,
// interference, sections and shape comparison on OCCT shapes.
//
// All lengths are millimetres and angles radians. Functions throw
// mitcad::analysis::Error for input they cannot analyse (for example a
// curved face where a planar one is needed) or when OCCT fails.

#include <stdexcept>
#include <vector>

#include <GProp_GProps.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>

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
// every volume and area MitCad reports, so that they agree. Each face is
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
// their exact values, and lofts with their shells and offsets within 1e-8
// of a reference: OCCT's adaptive Gauss-Kronrod rule for the curved faces,
// a dense polygon through the boundary curves for the plane ones
// (mitcad#1200, geometry.accurate_volumes). OCCT's own rules, even the
// adaptive ones, integrate along the boundary curves alone: where the
// curves' ends lie apart in the parameter plane (an offset's cap bounded
// by intersection curves of tolerance 8e-5 mm, ends 8.5e-5 mm apart) each
// gap leaves out the strip between it and the face's first parameter, 3e-6
// of a 1 mm shell of a loft; close_gaps here joins them (mitcad#325).
//
// Every volume, area, centre and mass MitCad reports comes from here: the
// bodies query, Physical Properties and the mass, the BOM, the print
// estimates, the imports' reports, measured areas and the section caps'
// area (geometry::mass_properties, geometry::faces_area). OCCT's default
// call is left only where a sign or an order is wanted (whether a solid is
// inside out, which piece lies further along) and in
// geometry::fixed_point_properties, which measures as other programs on
// OCCT do, for comparing with them.
//
// Cost: the number of points is fixed by the geometry, not by a tolerance,
// so a body always measures the same. A face takes (pieces across) x
// (pieces along its boundary curves) x points^2 evaluations: a piece per
// span of the surface and the curves (a rational span in 4), an angle
// parameter in at most 1024 steps per stretch, at most 30 points per
// piece. Large bodies are integrated face by face on all cores, and
// geometry::Shape keeps the measures, so a body is measured once. There is
// no time limit with a coarser fallback: a value that depended on the
// machine's load would not be the same twice.
//
// `only_closed`: closed shells only; `skip_shared`: a face shared by two
// shells once.
GProp_GProps volume_properties(const TopoDS_Shape& shape, bool only_closed = false,
                               bool skip_shared = false);
GProp_GProps surface_properties(const TopoDS_Shape& shape, bool skip_shared = false);

// The volume integral of the faces (each by its orientation) about `at`,
// as volume_properties integrates a shape's faces: the faces of a closed
// shell give its volume about any point; a part of them, the share they
// contribute about `at` (mitcad#235: the volume a change makes, from the
// faces it changed).
GProp_GProps faces_volume(const std::vector<TopoDS_Face>& faces, const gp_Pnt& at);

} // namespace mitcad::analysis
