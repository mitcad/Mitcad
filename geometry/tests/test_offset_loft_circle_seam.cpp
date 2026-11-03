// SPDX-License-Identifier: MIT
// Closed inward offsets of lofts from a polygon to a circle whose seam
// splits a side of the polygon (mitcad#1199), with OCCT alone. OCCT's loft
// (ThruSections) splits the side the circle's seam falls on at its middle,
// so the side becomes two faces tangent all along the edge between them.
// Their offsets share that edge's offset; at each end the offset of the
// end face is cut by both, in two curves tangent where they meet, and
// their intersection there was ill-conditioned: a vertex micrometres off
// along them that, fused with the one found where each crosses the shared
// edge, kept parameters out of its tolerance (the offset invalid), or none
// at all. With the port's patch 1160 those two curves are not intersected
// with each other there: their common vertex is where each crosses the
// offset of the tangential edge. The offsets are valid single solids, the
// inner walls the thickness from the outer ones, of the volume of the same
// loft with the seam on a corner (no side split).
//
// For a square to a circle of radius 20 the offset was valid, but the
// shared edge's curve on one half's offset was 0.13 mm off along it (its
// tolerance raised to 0.14 to cover it), that half 3.27 mm2 short and the
// volume 12.7 mm3 short at 0.5 mm (mitcad#1201): extending the edge
// (BRepOffset_Inter2d::ExtentEdge) reprojected that curve onto the half's
// surface, the projection failed, and SameParameter then replaced the
// exact line by a wrong approximation. With the port's patch 1180 the curve
// is kept where it lies on the edge all along the edge's own range.

#include <algorithm>
#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>

#include "check.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

// The octagon of mitcad#1184: sides on x = 17 and 47, y = -15 and 15, and
// the 45 degree sides between, at z = 72; the circle about (32, 0) at
// z = 228. Its sides are all as long, so the loft's faces are the same
// wherever the circle's seam lies: on a side (0 degrees, the middle of the
// side x = 47) or on a corner (22.5 degrees).
const double kOctagon[8][2] = {{47.0, 6.213203435596426},  {47.0, -6.213203435596426},
                               {38.21320343559643, -15.0}, {25.786796564403573, -15.0},
                               {17.0, -6.213203435596426}, {17.0, 6.213203435596426},
                               {25.786796564403573, 15.0}, {38.21320343559643, 15.0}};

// Volumes and areas as MitCad reports them (mitcad#1200's integrals), not
// OCCT's: OCCT's integration, even with an accuracy, leaves out a strip at
// each gap between the ends of an offset cap's intersection curves, and
// how much depends on the platform (on Windows the seam-on-a-corner offset
// of the square to a circle of radius 15 at 0.5 mm came out 3.4e-5 over
// the seam-on-a-side one, where MitCad's integrals of the two agree within
// 1e-9).
double volume_of(const TopoDS_Shape& shape) {
  return mitcad::geometry::mass_properties(mitcad::geometry::Shape(shape)).volume;
}

double area_of(const TopoDS_Shape& shape) {
  return mitcad::geometry::mass_properties(mitcad::geometry::Shape(shape)).area;
}

TopoDS_Wire circle_wire(const gp_Pnt& centre, double radius, double seam) {
  const gp_Ax2 axes(centre, gp_Dir(0, 0, 1), gp_Dir(std::cos(seam), std::sin(seam), 0));
  return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(gp_Circ(axes, radius)).Edge()).Wire();
}

TopoDS_Shape thru(const TopoDS_Wire& bottom, const TopoDS_Wire& top) {
  BRepOffsetAPI_ThruSections maker(true, false, 1.0e-6);
  maker.CheckCompatibility(true);
  maker.AddWire(bottom);
  maker.AddWire(top);
  maker.Build();
  CHECK(maker.IsDone());
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The octagon to the circle of `radius`, its seam at `seam` degrees.
TopoDS_Shape octagon_loft(double radius, double seam) {
  BRepBuilderAPI_MakePolygon octagon;
  for (const auto& p : kOctagon) {
    octagon.Add(gp_Pnt(p[0], p[1], 72.0));
  }
  octagon.Close();
  return thru(octagon.Wire(), circle_wire(gp_Pnt(32, 0, 228.0), radius, seam * kPi / 180.0));
}

// A 20 mm square at z = 0 to a circle of `radius` at z = 40, its seam at
// `seam` degrees: 0 is the middle of the side x = 10, 45 its corner.
TopoDS_Shape square_loft(double radius, double seam) {
  BRepBuilderAPI_MakePolygon square(gp_Pnt(-10, -10, 0), gp_Pnt(10, -10, 0), gp_Pnt(10, 10, 0),
                                    gp_Pnt(-10, 10, 0), true);
  return thru(square.Wire(), circle_wire(gp_Pnt(0, 0, 40), radius, seam * kPi / 180.0));
}

bool is_cap(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face);
  return surface.GetType() == GeomAbs_Plane &&
         std::abs(std::abs(surface.Plane().Axis().Direction().Z()) - 1.0) < 1e-9;
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// The closed inward offset of `body` by t, with OCCT alone; null where not
// done.
TopoDS_Shape closed_offset(const char* name, const TopoDS_Shape& body, double t) {
  BRepOffsetAPI_MakeOffsetShape maker;
  try {
    OCC_CATCH_SIGNALS
    maker.PerformByJoin(body, -t, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
  } catch (const Standard_Failure& error) {
    std::printf("%s: %s\n", name, error.what());
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The offset of `body` (its seam splitting a side) is a valid single solid;
// its inner walls lie t from the outer ones (at each inner face's middle,
// within 1e-6 mm); its edges are within 0.01 mm of their faces; its volume
// lies between the body's less its area times t and the body's less the
// offset's area times t, and is that of the offset of `corner` (the same
// loft with the seam on a corner) where that one is valid. The body is
// left as it was. `edges` and `relative` tighten the last two.
void check_closed_offset(const char* name, const TopoDS_Shape& body, const TopoDS_Shape& corner, double t,
                         double edges = 0.01, double relative = 3e-5) {
  const double       before = volume_of(body);
  const TopoDS_Shape result = closed_offset(name, body, t);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    std::printf("%s: not done\n", name);
    return;
  }
  CHECK(std::abs(volume_of(body) - before) <= 1e-9 * before);
  const bool valid = BRepCheck_Analyzer(result).IsValid();
  CHECK(valid);
  CHECK(count(result, TopAbs_SOLID) == 1);
  const double v = volume_of(result), V = volume_of(body);
  const double s = area_of(result), S = area_of(body);
  double       tolerance = 0.0;
  for (TopExp_Explorer e(result, TopAbs_EDGE); e.More(); e.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
  }
  std::printf("%s: valid %d, volume %.6f in [%.6f, %.6f], edges within %.3g\n", name, (int)valid, v, V - S * t,
              V - s * t, tolerance);
  CHECK(v >= V - S * t && v <= V - s * t);
  CHECK(tolerance <= edges);
  const TopoDS_Shape reference = closed_offset(name, corner, t);
  if (!reference.IsNull() && BRepCheck_Analyzer(reference).IsValid()) {
    std::printf("%s: the seam on a corner gives %.6f\n", name, volume_of(reference));
    CHECK_NEAR_TOL(v, volume_of(reference), relative);
  }
  BRep_Builder    builder;
  TopoDS_Compound sides;
  builder.MakeCompound(sides);
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    if (!is_cap(TopoDS::Face(f.Current()))) {
      builder.Add(sides, f.Current());
    }
  }
  int inner = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    if (is_cap(face)) {
      continue;
    }
    ++inner;
    double u0, u1, v0, v1;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const gp_Pnt p = BRepAdaptor_Surface(face).Value(0.5 * (u0 + u1), 0.5 * (v0 + v1));
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), sides);
    CHECK(distance.IsDone());
    if (distance.IsDone() && std::abs(distance.Value() - t) > 1e-6) {
      std::printf("%s: an inner face lies %.9f from the outer walls, not %g\n", name, distance.Value(), t);
      CHECK(false);
    }
  }
  CHECK(inner == count(sides, TopAbs_FACE));
}

void test_square_to_a_circle_seam_on_a_side() {
  const struct {
    double radius, t;
  } cases[] = {{15.0, 0.5}, {15.0, 1.0}, {15.0, 2.0}, {15.0, 3.0}, {14.142, 2.0}};
  for (const auto& c : cases) {
    char name[96];
    std::snprintf(name, sizeof name, "square to a circle of radius %g, seam on a side, offset %g", c.radius, c.t);
    check_closed_offset(name, square_loft(c.radius, 0.0), square_loft(c.radius, 45.0), c.t);
  }
}

// The square to a circle of radius 20 (mitcad#1201): edges within
// 0.001 mm, the volume that of the seam on a corner within 1e-5 (at 2 mm
// the offset of the seam-on-a-corner loft is invalid and not compared).
void test_square_to_a_circle_of_radius_20_seam_on_a_side() {
  for (const double t : {0.5, 1.0, 2.0}) {
    char name[96];
    std::snprintf(name, sizeof name, "square to a circle of radius 20, seam on a side, offset %g", t);
    check_closed_offset(name, square_loft(20.0, 0.0), square_loft(20.0, 45.0), t, 1e-3, 1e-5);
  }
}

void test_octagon_to_a_circle_seam_on_a_side() {
  for (const double t : {0.5, 1.0, 2.0, 3.0}) {
    char name[96];
    std::snprintf(name, sizeof name, "octagon to a circle of radius 21.7, seam on a side, offset %g", t);
    check_closed_offset(name, octagon_loft(21.7, 0.0), octagon_loft(21.7, 22.5), t);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_square_to_a_circle_seam_on_a_side", test::test_square_to_a_circle_seam_on_a_side);
  test::guarded("test_octagon_to_a_circle_seam_on_a_side", test::test_octagon_to_a_circle_seam_on_a_side);
  test::guarded("test_square_to_a_circle_of_radius_20_seam_on_a_side",
                test::test_square_to_a_circle_of_radius_20_seam_on_a_side);
  if (test::failures == 0) {
    std::puts("test_offset_loft_circle_seam: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
