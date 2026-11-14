// SPDX-License-Identifier: MIT
#pragma once

// Whether shapes come within a distance of each other, and whether points
// lie on a surface, asked again and again by the dressups' checks and
// fallbacks (mitcad#1785): the same answers as BRepExtrema's distance and a
// projection on the surface give, without measuring more than the answer
// needs.

#include <array>
#include <memory>
#include <optional>
#include <vector>

#include <BRepClass3d_SolidClassifier.hxx>
#include <Bnd_Box.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_Handle.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopAbs_State.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

namespace mitcad::geometry::detail {

// A shape taken apart as BRepExtrema_DistShapeShape takes it: its
// vertices, edges and faces, each with its bounding box. Made once for a
// shape that is measured against many others; a solid also keeps its
// classifier and the vertices it has classified.
class Proximity {
public:
  explicit Proximity(const TopoDS_Shape& of);
  Proximity(const Proximity&) = delete;
  Proximity& operator=(const Proximity&) = delete;
  ~Proximity();

  using Map = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;

private:
  friend bool within_distance(const Proximity& a, const Proximity& b, double limit);
  // Whether a vertex of `points` lies inside this solid, as
  // BRepExtrema_DistShapeShape classifies it.
  bool holds_vertex_of(const Proximity& points) const;

  TopoDS_Shape shape_;
  bool solid_;
  Map vertices_, edges_, faces_;
  std::vector<Bnd_Box> vertex_boxes_, edge_boxes_, face_boxes_;
  // The boxes of the Bezier pieces of a B-spline or Bezier edge's curve
  // and face's surface, each holding its piece (none for other curves and
  // surfaces): a helix's box holds the whole thread, its pieces' a turn's
  // part each.
  std::vector<std::vector<Bnd_Box>> edge_pieces_, face_pieces_;
  // The poles of those pieces of an edge's curve (homogeneous: x w, y w,
  // z w, w), so that pairs of edges whose pieces' boxes come near are told
  // apart by the boxes of the pieces' halves (mitcad#1796), and the edge's
  // tolerance.
  using Hull = std::vector<std::array<double, 4>>;
  std::vector<std::vector<Hull>> edge_hulls_;
  std::vector<double> edge_tolerances_;
  Bnd_Box bounds_; // a solid's, grown by its classifier's tolerance
  mutable std::unique_ptr<BRepClass3d_SolidClassifier> classifier_;
  mutable NCollection_DataMap<TopoDS_Shape, bool, TopTools_ShapeMapHasher> inside_;
};

// Whether BRepExtrema_DistShapeShape's distance between the shapes is at
// most `limit` (a vertex of one inside the other solid counting as no
// distance, as there): the same measures of the same pairs of vertices,
// edges and faces, with the same boxes, but stopping at the first pair
// within the limit and never measuring pairs whose boxes are farther apart
// than it. The full distance measures every pair whose boxes come nearer
// than the nearest pair found so far, also when the shapes touch: between
// the helical edges of a modelled thread a few such measures take seconds.
// False where the distance finds nothing.
bool within_distance(const Proximity& a, const Proximity& b, double limit);
bool within_distance(const TopoDS_Shape& a, const TopoDS_Shape& b, double limit);

// A surface points are projected on again and again: whether points lie
// on it as a projection of each (GeomAPI_ProjectPointOnSurf on the whole
// surface) tells, the projection set up once for all of them, and a point
// outside the box of a bounded surface's poles (which holds the surface)
// not projected at all.
class SurfaceProbe {
public:
  explicit SurfaceProbe(const occ::handle<Geom_Surface>& surface);
  SurfaceProbe(SurfaceProbe&&) noexcept;
  SurfaceProbe& operator=(SurfaceProbe&&) noexcept;
  ~SurfaceProbe();

  // Whether every point lies on the surface within `tolerance` (false for
  // no points or no surface).
  bool holds(const std::vector<gp_Pnt>& points, double tolerance);

private:
  occ::handle<Geom_Surface> surface_;
  Bnd_Box poles_; // void where the surface is not bounded
  std::unique_ptr<GeomAPI_ProjectPointOnSurf> projection_;
};

// Which side of a closed body's boundary a point lies on, as a ray from it
// first crosses the boundary (mitcad#1257).
enum class RaySide { Inside, Outside, Near };

// The least cosine of a ray's angle with the boundary's normal where it
// crosses it first that tells the side.
constexpr double kRayTransverse = 0.05;

// The side of `point` by the first crossing of the boundary (loaded into
// `boundary`) along a ray in `direction`: leaving the body there (along the
// face's outward normal), the point lies inside it; entering it, outside.
// Near where the boundary lies within `close` of the point along the ray:
// the point is then no farther than that from it on either side. None where
// the ray meets nothing up to `reach`, or meets the boundary at a tangent
// or on a face's edge: a solid classifier tells then. (A classifier's rays
// run through the whole body: on a body of long spline faces, a thread's,
// it took 60 ms a point in a debug build, 40 times as long as the rounding
// it checked; a short ray meets only the faces near the point.)
std::optional<RaySide> ray_side(IntCurvesFace_ShapeIntersector& boundary, const gp_Pnt& point,
                                const gp_Dir& direction, double close, double reach);

// The side of `point` by rays the whole way through the body (`through`
// longer than its box's diagonal) in a few fixed directions skew to the
// axes (a body's faces and edges mostly run along them), the first that
// tells (ray_side): Inside or Outside; Near where one finds the boundary
// within `tolerance` / kRayTransverse of the point (it may lie within
// `tolerance` of it, on it for a classifier); none where none tells
// (mitcad#1796).
std::optional<RaySide> skew_ray_side(IntCurvesFace_ShapeIntersector& boundary, const gp_Pnt& point,
                                     double tolerance, double through);

// A ray length that runs through the whole of a shape from any point in its
// box (twice its box's diagonal); 0 for an empty shape.
double length_through(const TopoDS_Shape& shape);

// Which side of a closed body's boundary points lie on, as
// BRepClass3d_SolidClassifier tells, rays through the body asked first
// (skew_ray_side), the classifier only where they cannot tell
// (mitcad#1796): a classifier measures its ray against every edge whose box
// it crosses, and a modelled thread's or a loft's spline edges span the
// body, tens of milliseconds a point. Both are made once, when first
// needed.
class PointSide {
public:
  explicit PointSide(const TopoDS_Shape& body);
  PointSide(const PointSide&) = delete;
  PointSide& operator=(const PointSide&) = delete;
  ~PointSide();

  // IN, OUT, or ON within `tolerance` of the boundary, as
  // BRepClass3d_SolidClassifier(body).Perform(point, tolerance) gives it
  // (rays tell IN or OUT only for a point farther than `tolerance` from
  // the faces they cross, which all but a point that near an edge is).
  TopAbs_State state(const gp_Pnt& point, double tolerance);

private:
  TopoDS_Shape body_;
  std::unique_ptr<IntCurvesFace_ShapeIntersector> rays_;
  double through_ = 0.0;
  std::unique_ptr<BRepClass3d_SolidClassifier> classifier_;
};

} // namespace mitcad::geometry::detail
