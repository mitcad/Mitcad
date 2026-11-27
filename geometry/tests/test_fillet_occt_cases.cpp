// SPDX-License-Identifier: MIT
// Roundings and a chamfer of simple bodies, built here with OCCT's fillet
// alone, that OCCT 8.0.1 builds and that five patches of the OCCT port
// (third_party/vcpkg-ports) once broke: an invalid body, a tolerance over
// 1e-4 or no result (mitcad#1883, found by running OCCT's own test cases
// against the port, mitcad#1875). Each test fails with the patch as it was.

#include <cmath>
#include <cstdio>
#include <functional>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfEllipse.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Elips.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The edges of a shape whose middle point passes the test.
std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<TopoDS_Edge>                                      edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge&            edge  = TopoDS::Edge(all(i));
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (!curve.IsNull() && keep(curve->Value(0.5 * (first + last)))) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// The largest tolerance of the shape's edges and vertices.
double max_tolerance(const TopoDS_Shape& shape) {
  double tol = 0.0;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    tol = std::max(tol, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
  }
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    tol = std::max(tol, BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())));
  }
  return tol;
}

// Builds a fillet or chamfer; a null shape when it fails.
template <typename Maker>
TopoDS_Shape built(Maker& maker, const char* what) {
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s: %s\n", what, failure.what());
    return TopoDS_Shape();
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The edges rounded with radius r; a null shape when it fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : edges) {
    fillet.Add(r, edge);
  }
  return built(fillet, "fillet");
}

bool valid(const TopoDS_Shape& shape) {
  return !shape.IsNull() && BRepCheck_Analyzer(shape).IsValid();
}

// A profile in the plane y = 0 revolved by 270 degrees about z: a quarter
// ellipse from (150, 0, 0) up to (0, 0, 100) (half axes 150 and 100), the
// axis down to (0, 0, -100), out to (250, 0, -100) and back to (150, 0, 0).
// The top face is a surface of revolution of the ellipse, its pole at
// (0, 0, 100); the end planes are x = 0 (y < 0) and y = 0 (x > 0).
TopoDS_Shape revolved_ellipse() {
  const gp_Elips                 ellipse(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, -1, 0), gp_Dir(1, 0, 0)), 150, 100);
  const occ::handle<Geom_Curve>  arc(GC_MakeArcOfEllipse(ellipse, 0.0, kPi / 2.0, true).Value());
  BRepBuilderAPI_MakePolygon     rest(gp_Pnt(0, 0, 100), gp_Pnt(0, 0, -100), gp_Pnt(250, 0, -100), gp_Pnt(150, 0, 0));
  BRepBuilderAPI_MakeWire        wire;
  wire.Add(BRepBuilderAPI_MakeEdge(arc).Edge());
  wire.Add(rest.Wire());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return BRepPrimAPI_MakeRevol(face, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 1.5 * kPi).Shape();
}

// Port patch 0211: the axis edge, concave (the end planes meet at 270
// degrees), rounded with 10 up to the pole of the ellipse's surface of
// revolution, the face at that end. The face is periodic but covers 270
// degrees only: 0211 moved the second contact's point on it by a period,
// for a seam the face does not have, and the body came out invalid. (The
// corner there, OCCT's, is valid but not the true rounding: the volume is
// below the unrounded body's. The test checks validity only.)
void test_axis_edge_rounded_up_to_the_pole_of_a_partial_revolve() {
  const TopoDS_Shape             body = revolved_ellipse();
  const std::vector<TopoDS_Edge> axis =
    edges_where(body, [](const gp_Pnt& m) { return std::abs(m.X()) < 1e-7 && std::abs(m.Y()) < 1e-7; });
  CHECK(axis.size() == 1);
  CHECK(valid(rounded(body, axis, 10.0)));
  // The same with a radius running 10, 20, 10 along the edge.
  BRepFilletAPI_MakeFillet fillet(body);
  if (axis.size() == 1) {
    NCollection_Array1<gp_Pnt2d> law(1, 3);
    law(1) = gp_Pnt2d(0.0, 10.0);
    law(2) = gp_Pnt2d(0.5, 20.0);
    law(3) = gp_Pnt2d(1.0, 10.0);
    fillet.Add(law, axis.front());
  }
  CHECK(valid(built(fillet, "evolving fillet")));
}

// Port patch 0721: the top's rim (the circle of radius 150 at z = 0), the
// ellipse's edge with the end plane x = 0 and the cone's edge with that
// plane rounded with 10. The ellipse's edge ends at the pole; 0721 continued
// its curve past the pole for the end's prolongation (the ellipse lies on
// both surfaces there, its curve on the surface of revolution past the
// meridian's range) and took the pole's point on the top a period away: the
// body came out invalid.
void test_meridian_edge_rounded_to_the_pole_of_a_partial_revolve() {
  const TopoDS_Shape             body  = revolved_ellipse();
  std::vector<TopoDS_Edge>       edges = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.Z()) < 1e-7 && std::abs(std::hypot(m.X(), m.Y()) - 150.0) < 1e-6;
  });
  const std::vector<TopoDS_Edge> side  = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.X()) < 1e-7 && m.Y() < -1.0 && m.Z() > -99.0;
  });
  CHECK(edges.size() == 1);
  CHECK(side.size() == 2);
  edges.insert(edges.end(), side.begin(), side.end());
  CHECK(valid(rounded(body, edges, 10.0)));
}

// Port patch 0153 (its pair rule as 0270 has it): a box 100 wide whose two
// top edges along x and two vertical edges at their ends are rounded with
// 50, the top a full round. 0153 let the two top roundings meet on their
// common contact line and the top vanish, and the corners with the vertical
// roundings built round that face made an invalid body. Now: valid, and the
// volume of the full round and the vertical roundings each cut by the
// other, integrated apart (809366.23).
void test_box_top_full_round_with_rounded_vertical_edges() {
  const TopoDS_Shape             body  = BRepPrimAPI_MakeBox(100, 100, 100).Shape();
  const std::vector<TopoDS_Edge> edges = edges_where(body, [](const gp_Pnt& m) {
    const bool front_top   = std::abs(m.Y()) < 1e-7 && std::abs(m.Z() - 100) < 1e-7;
    const bool back_top    = std::abs(m.Y() - 100) < 1e-7 && std::abs(m.Z() - 100) < 1e-7;
    const bool front_right = std::abs(m.X() - 100) < 1e-7 && std::abs(m.Y()) < 1e-7;
    const bool back_left   = std::abs(m.X()) < 1e-7 && std::abs(m.Y() - 100) < 1e-7;
    return front_top || back_top || front_right || back_left;
  });
  CHECK(edges.size() == 4);
  const TopoDS_Shape result = rounded(body, edges, 50.0);
  CHECK(valid(result));
  if (valid(result)) {
    // The full round's prism less, for each vertical rounding, the region
    // outside its quarter circle within the full round's section.
    double removed = 0.0;
    const int n = 2000;
    for (int i = 0; i < n; ++i) {
      const double z  = (i + 0.5) * 100.0 / n;
      const double s  = z > 50.0 ? std::sqrt(std::max(0.0, 2500.0 - (z - 50.0) * (z - 50.0))) : 50.0;
      const double lo = std::max(0.0, 50.0 - s);
      for (int j = 0; j < n; ++j) {
        const double x  = 50.0 + (j + 0.5) * 50.0 / n;
        const double hi = 50.0 - std::sqrt(std::max(0.0, 2500.0 - (x - 50.0) * (x - 50.0)));
        if (hi > lo) {
          removed += (hi - lo) * (100.0 / n) * (50.0 / n);
        }
      }
    }
    const double expected = 100.0 * (5000.0 + kPi * 1250.0) - 2.0 * removed;
    CHECK_NEAR_TOL(volume_of(result), expected, 1e-6);
  }
}

// Port patch 0620: the 270-degree revolve of a profile with a groove (a cone
// from the apex (0, 0, 250) down to radius 200 at z = 50, in to radius 50,
// down to z = -50, out to 200 and a cone to (0, 0, -250)), the upper cone's
// edge with the end plane x = 0, its rim and the groove's top edge with that
// plane rounded with 10. The first section beside the apex put the cone's
// contact on its rim round the far side; 0620 took that for an edge the
// ball rolls on before the spine's end, and the rounding failed.
void test_cone_edge_rounded_from_its_apex_on_a_partial_revolve() {
  BRepBuilderAPI_MakePolygon profile;
  for (const gp_Pnt& p : {gp_Pnt(0, 0, 250), gp_Pnt(200, 0, 50), gp_Pnt(50, 0, 50), gp_Pnt(50, 0, -50),
                          gp_Pnt(200, 0, -50), gp_Pnt(0, 0, -250)}) {
    profile.Add(p);
  }
  profile.Close();
  const TopoDS_Face  face = BRepBuilderAPI_MakeFace(profile.Wire(), true).Face();
  const TopoDS_Shape body =
    BRepPrimAPI_MakeRevol(face, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 1.5 * kPi).Shape();
  const std::vector<TopoDS_Edge> edges = edges_where(body, [](const gp_Pnt& m) {
    const bool cone_side = std::abs(m.X()) < 1e-7 && m.Y() < -1.0 && m.Z() > 50.0 + 1e-6;
    const bool rim       = std::abs(m.Z() - 50.0) < 1e-7 && std::abs(std::hypot(m.X(), m.Y()) - 200.0) < 1e-6;
    const bool groove    = std::abs(m.X()) < 1e-7 && m.Y() < -1.0 && std::abs(m.Z() - 50.0) < 1e-7;
    return cone_side || rim || groove;
  });
  CHECK(edges.size() == 3);
  CHECK(valid(rounded(body, edges, 10.0)));
}

// Port patch 0771: two cylinders of radius 30 and length 150 crossing at
// right angles, their axes 1e-3 apart, fused, and the section edges
// rounded with 10. Where the two intersection curves come nearest, the
// cylinders cross at 0.008 rad, within the fillet's angular tolerance;
// 0771 stopped the contour there, as where a rounding vanishes, and the
// rounding failed. The roundings are concave: material is added. (The
// cylinders are made a tenth of the size, placed and scaled up by 10, the
// scaled surfaces as such a body has them: built at full size directly,
// the rounding fails with and without the patches, near those points.)
void test_crossing_cylinders_rounded_through_their_near_tangent_points() {
  gp_Trsf turn, shift, scale;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), kPi / 2.0);
  shift.SetTranslation(gp_Vec(0.0001, 7.5, 7.5));
  scale.SetScale(gp_Pnt(0, 0, 0), 10.0);
  TopoDS_Shape b = BRepPrimAPI_MakeCylinder(3, 15).Shape();
  b              = BRepBuilderAPI_Transform(b, turn).Shape();
  b              = BRepBuilderAPI_Transform(b, shift).Shape();
  b              = BRepBuilderAPI_Transform(b, scale).Shape();
  const TopoDS_Shape a = BRepBuilderAPI_Transform(BRepPrimAPI_MakeCylinder(3, 15).Shape(), scale).Shape();
  BRepAlgoAPI_Fuse fuse(a, b);
  CHECK(fuse.IsDone());
  const TopoDS_Shape       body = fuse.Shape();
  std::vector<TopoDS_Edge> section;
  for (const TopoDS_Shape& edge : fuse.SectionEdges()) {
    section.push_back(TopoDS::Edge(edge));
  }
  CHECK(!section.empty());
  const TopoDS_Shape result = rounded(body, section, 10.0);
  CHECK(valid(result));
  if (valid(result)) {
    CHECK(volume_of(result) > volume_of(body) + 1000.0);
  }
}

// Port patch 0721: a cylinder of radius 10 standing in a box's corner (the
// box's top at z = 20, its side at x = 5 cutting the cylinder), fused; the
// cylinder's two straight edges with the side chamfered as a throat with
// penetration (1 and 1.2). The ends' prolongations run along the box's top
// circle: 0721 made that circle's curve on the top a B-spline of another
// parametrization, and the edges' tolerance came out 1.5e-4. Now within
// 1e-4.
void test_throat_chamfer_ends_along_a_circle_keep_their_tolerance() {
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(10, 50).Shape();
  const TopoDS_Shape box      = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, -20), gp_Pnt(5, 20, 20)).Shape();
  BRepAlgoAPI_Fuse   fuse(cylinder, box);
  const TopoDS_Shape body = fuse.Shape();
  // The cylinder's side as the fuse left it, and its straight edges on x = 5.
  TopoDS_Face side;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cylinder) {
      side = TopoDS::Face(f.Current());
    }
  }
  const std::vector<TopoDS_Edge> lines =
    edges_where(body, [](const gp_Pnt& m) { return std::abs(m.X() - 5.0) < 1e-7 && std::abs(m.Z() - 10.0) < 1e-7; });
  CHECK(!side.IsNull());
  CHECK(lines.size() == 2);
  BRepFilletAPI_MakeChamfer chamfer(body);
  chamfer.SetMode(ChFiDS_ConstThroatWithPenetrationChamfer);
  for (const TopoDS_Edge& edge : lines) {
    chamfer.Add(1.0, 1.2, edge, side);
  }
  const TopoDS_Shape result = built(chamfer, "chamfer");
  CHECK(valid(result));
  if (valid(result)) {
    CHECK(max_tolerance(result) <= 1e-4);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_occt_cases) {
  test::guarded("test_axis_edge_rounded_up_to_the_pole_of_a_partial_revolve",
                test::test_axis_edge_rounded_up_to_the_pole_of_a_partial_revolve);
  test::guarded("test_meridian_edge_rounded_to_the_pole_of_a_partial_revolve",
                test::test_meridian_edge_rounded_to_the_pole_of_a_partial_revolve);
  test::guarded("test_box_top_full_round_with_rounded_vertical_edges",
                test::test_box_top_full_round_with_rounded_vertical_edges);
  test::guarded("test_cone_edge_rounded_from_its_apex_on_a_partial_revolve",
                test::test_cone_edge_rounded_from_its_apex_on_a_partial_revolve);
  test::guarded("test_crossing_cylinders_rounded_through_their_near_tangent_points",
                test::test_crossing_cylinders_rounded_through_their_near_tangent_points);
  test::guarded("test_throat_chamfer_ends_along_a_circle_keep_their_tolerance",
                test::test_throat_chamfer_ends_along_a_circle_keep_their_tolerance);
  if (test::failures == 0) {
    std::puts("test_fillet_occt_cases: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
