// SPDX-License-Identifier: MIT
#pragma once

// Builds OCCT shapes from the neutral B-rep model of an imported body (see
// core/asm/src/brep.rs and flat.rs). Lengths are millimetres.
//
// Conventions of the model: an edge runs along increasing parameter of its
// curve from v0 (t0) to v1 (t1); a coedge uses its edge forward or reversed;
// loops have the face on their left when viewed against the face's material
// normal, which is the surface's natural normal unless `reversed` is set.
//
// Curves and surfaces are a kind plus a slice of `ints` and `reals`:
//   curve 0 line          reals: origin(3) dir(3)
//   curve 1 ellipse       reals: center(3) normal(3) major_dir(3) major minor
//   curve 2 bspline       ints: degree n_knots n_poles rational periodic mults...
//                         reals: knots... poles(3 each)... weights...
//   curve 3 interpolated  ints: n_points; reals: params... points(3 each)...
//   curve 4 on surface    ints: surface_kind surface_ints... degree n_knots
//                               n_poles rational mults...;
//                         reals: surface_reals... knots... poles(2 each)...
//                                weights...
//                         the surface along the 2D B-spline in its parameters
//   surface 0 plane      reals: origin(3) normal(3) u_dir(3)
//   surface 1 cone        reals: origin(3) axis(3) ref_dir(3) radius half_angle
//                         (cylinder when half_angle is 0)
//   surface 2 sphere      reals: center(3) axis(3) ref_dir(3) radius
//   surface 3 torus       reals: center(3) axis(3) ref_dir(3) major minor
//   surface 4 bspline     ints: u_deg v_deg n_uk n_vk nu nv rational
//                               u_periodic v_periodic u_mults... v_mults...
//                         reals: u_knots... v_knots... poles(3 each, u-major)...
//                                weights...
//   surface 5 extrusion   ints: curve_kind curve_ints...; reals: dir(3) curve_reals...
//   surface 6 revolution  ints: curve_kind curve_ints...;
//                         reals: origin(3) axis(3) curve_reals...
//   surface 7 ruled       ints: from_kind from_ints... to_kind to_ints...;
//                         reals: from_reals... to_reals...
//                         S(u, v) = (1 - u) from(v) + u to(v), u in [0, 1];
//                         both curves B-splines with the same knots
//   surface 8 arc sweep   ints: n (kind ints...) n times;
//                         reals: weights(n) reals... n times;
//                         rational quadratic arcs in u (knots spaced evenly
//                         in [0, 1], one arc per two spans) whose control
//                         points are the n curves' points at v, all
//                         polynomial B-splines with the same knots
//   surface 9 rolling ball  ints: (is_curve kind ints...) twice,
//                               spine_kind spine_ints...;
//                         reals: radius v0 v1 (support reals...) twice,
//                                spine_reals...;
//                         the ball of the radius whose centre runs along
//                         the spine over [v0, v1] touching both supports
//                         (surfaces, or curves where is_curve), walked by
//                         OCCT and approximated (blend_surfaces.hpp)
//   curve 5 unknown       no data: an edge the reader could not decode; the
//                         builder leaves it out (not counted as failed)
//   surface 10 offset     ints: basis_kind basis_ints...;
//                         reals: distance basis_reals...;
//                         Geom_OffsetSurface of the basis
//   surface 11 unknown    no data: a face the reader could not decode; the
//                         builder leaves it out (not counted as failed)
//
// A face's point loops (Body::point_loops[first_point_loop ...]) are
// vertices where the face closes up at a singular point of its surface
// (a cone apex); the builder bounds the face there with a degenerated edge.

#include <array>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <string>
#include <vector>

#include <TopoDS_Shape.hxx>

namespace mitcad::brep {

struct Geometry {
  int kind = 0;
  std::uint32_t int_offset = 0;
  std::uint32_t int_count = 0;
  std::uint32_t real_offset = 0;
  std::uint32_t real_count = 0;
};

struct Edge {
  std::uint32_t curve = 0;
  std::uint32_t v0 = 0;
  std::uint32_t v1 = 0;
  double t0 = 0.0;
  double t1 = 0.0;
  double tolerance = 0.0;
};

struct Face {
  std::uint32_t surface = 0;
  bool reversed = false;
  bool double_sided = false;
  std::uint32_t first_loop = 0;
  // A face without loops or point loops on a bounded surface (a B-spline)
  // is the whole surface.
  std::uint32_t loop_count = 0;
  std::uint32_t first_point_loop = 0; // range in Body::point_loops
  std::uint32_t point_loop_count = 0;
};

struct Loop {
  std::uint32_t first_coedge = 0;
  std::uint32_t coedge_count = 0;
};

struct Coedge {
  std::uint32_t edge = 0;
  bool forward = true;
};

struct Shell {
  std::uint32_t lump = 0;
  std::uint32_t first_face = 0; // range in Body::shell_faces
  std::uint32_t face_count = 0;
  bool closed = false;
};

struct Body {
  std::vector<std::int32_t> ints;
  std::vector<double> reals;
  std::vector<Geometry> curves;
  std::vector<Geometry> surfaces;
  std::vector<double> vertices; // x, y, z, tolerance per vertex
  std::vector<Edge> edges;
  std::vector<Face> faces;
  std::vector<Loop> loops;
  std::vector<Coedge> coedges;
  std::vector<Shell> shells;
  std::vector<std::uint32_t> shell_faces;
  std::vector<std::uint32_t> point_loops; // vertex indices
  std::uint32_t lump_count = 0;
  std::vector<double> transform; // empty, or 3x3 rows then translation
};

// What the build of a body keeps for the builds of its other states
// (mitcad#181: the states of a body's history share most of their faces,
// and healing a body of thousands of faces takes tens of seconds): its
// healed faces, edges and vertices by the content of their data. Immutable
// once built; builds on other threads may read it at once.
struct Healed;

// Bodies of at least this many faces keep and reuse what was healed.
constexpr std::size_t kMinReuseFaces = 256;

struct BuildOptions {
  // Minimum tolerance of vertices and edges (mm).
  double tolerance = 1e-4;
  // Run ShapeFix (pcurves, seams, same-parameter, tolerances).
  bool fix = true;
  // Fill the report's volumes, area, box and validity (costly on large
  // bodies; the .f3d importer measures the bodies itself).
  bool measure = true;
  // Called as the build advances (ShapeFix face by face), at most ten
  // thousand times: healing a body of tens of thousands of faces takes
  // minutes, which a caller watching for hangs must not take for one (the
  // .f3d import's watchdog, mitcad#82). Not called while a step of OCCT's
  // does not return. May be empty.
  std::function<void()> progress;
  // Keep what the builds of the body's other states can take from this one
  // (BuildResult::healed; a valid solid of at least kMinReuseFaces faces).
  bool keep = false;
  // What builds of other states of the body kept (BuildResult::healed):
  // the faces whose data is the same as in the one that shares the most
  // with this body are taken as healed there (copies), and only the others
  // are healed. When less than half of the faces are shared, or the result
  // is not valid, the body is built and healed as a whole.
  std::vector<std::shared_ptr<const Healed>> bases;
  // The smallest body (faces) that keeps and reuses what was healed.
  std::size_t reuse_min_faces = kMinReuseFaces;
};

struct BuildReport {
  bool built = false;
  std::string error;
  // An allocation failed while the body was built (mitcad#132), also one
  // OCCT's healing or checker caught inside, on this thread or in the jobs
  // of OCCT's thread pool it ran: not built, error "out of memory".
  bool out_of_memory = false;
  // Every lump became a solid (closed shells only).
  bool solid = false;
  // BRepCheck_Analyzer accepts the result.
  bool valid = false;
  // Volume before orienting the solids: negative when the material is on
  // the wrong side of the faces.
  double raw_volume = 0.0;
  double volume = 0.0;
  double area = 0.0;
  std::array<double, 6> bbox{}; // xmin ymin zmin xmax ymax zmax
  int faces = 0;
  int edges = 0;
  int vertices = 0;
  // Curves, surfaces, edges and faces that could not be built.
  int curves_failed = 0;
  int surfaces_failed = 0;
  int edges_failed = 0;
  int faces_failed = 0;
  // Faces taken as healed from BuildOptions::bases (0: healed as a whole).
  int reused_faces = 0;
  std::vector<std::string> messages;
};

struct BuildResult {
  TopoDS_Shape shape;
  BuildReport report;
  // With BuildOptions::keep: for the builds of the body's other states
  // (null when the body is not kept).
  std::shared_ptr<const Healed> healed;
};

// Never throws; failures are reported in BuildReport.
BuildResult build_body(const Body& body, const BuildOptions& options = {});

} // namespace mitcad::brep
