// SPDX-License-Identifier: MIT
// A rounding ending on a cylinder whose seam runs into the rounding's end
// vertex, built with OCCT's fillet alone: a roof whose ridge runs into a
// rod standing on it, the rod's seam along the line through the ridge's
// end, the ridge rounded. The rounding takes material off the ridge, so the
// rod's face grows down to the rounding's cut, which crosses the seam's
// line below the vertex where the seam ended: the seam must run on to the
// cut (the rod's seam a B-spline curve, which cannot be prolonged, and away
// from its surface's u = 0, as on the stored design). A node of a stored
// design, where roundings of the curves between three bores end on a rod
// standing in their middle, is the case behind the patch 0931 of the OCCT
// port (third_party/vcpkg-ports, mitcad#343), which splits the cut at the
// seam's line and runs the seam on along it. On this body OCCT's
// topological build closes the face without it as well; on the design's it
// did not (the rod's face came out with a wire not closed), so the test
// checks the result of 0931's way, not its failure without it.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeFix_Solid.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

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

constexpr double kHalf = 10.0;  // the roof's half width (x) and half length (y)
constexpr double kRidge = 5.0;  // the ridge's height over the eaves
constexpr double kRod = 2.5;    // the rod's radius
constexpr double kRound = 1.0;  // the ridge's rounding

// The roof's cross-section in the plane y = y0: the eaves at z = 0, the
// ridge at (0, kRidge).
TopoDS_Face roof_section(double y0) {
  BRepBuilderAPI_MakePolygon triangle(gp_Pnt(-kHalf, y0, 0), gp_Pnt(kHalf, y0, 0), gp_Pnt(0, y0, kRidge), true);
  return BRepBuilderAPI_MakeFace(triangle.Wire(), true).Face();
}

// The rod, its seam along the line x = 0, y = kRod (through where the
// ridge meets it).
TopoDS_Shape rod(double seam) {
  // The cylinder's face from u = pi / 2 to 5 pi / 2 (its seam there, not
  // at the surface's u = 0, as on the stored design's rod), closed by its
  // end discs.
  const occ::handle<Geom_CylindricalSurface> surface =
    new Geom_CylindricalSurface(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), kRod);
  const TopoDS_Face side = BRepBuilderAPI_MakeFace(surface, seam, seam + 2.0 * kPi, 0.0, 3.0 * kRidge, 1e-7).Face();
  BRepBuilderAPI_Sewing sewing(1e-6);
  sewing.Add(side);
  for (const double z : {0.0, 3.0 * kRidge}) {
    const gp_Circ circle(gp_Ax2(gp_Pnt(0, 0, z), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), kRod);
    sewing.Add(BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circle).Edge()).Wire(), true)
                 .Face());
  }
  sewing.Perform();
  BRepBuilderAPI_MakeSolid solid(TopoDS::Shell(sewing.SewedShape()));
  ShapeFix_Solid fix(solid.Solid());
  fix.Perform();
  return fix.Solid();
}

// The roof and the rod joined. The rod's seam gets its line as a B-spline
// curve, as the stored design's seam is (a line can be prolonged past its
// edge, a B-spline curve not).
TopoDS_Shape roof_with_a_rod(double seam) {
  const TopoDS_Shape roof = BRepPrimAPI_MakePrism(roof_section(-kHalf), gp_Vec(0, 2.0 * kHalf, 0)).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Fuse(roof, rod(seam)).Shape();
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      double first = 0.0, last = 0.0;
      const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
      if (BRep_Tool::IsClosed(edge, face) && !curve.IsNull() && !curve->IsKind(STANDARD_TYPE(Geom_BSplineCurve))) {
        BRep_Builder().UpdateEdge(edge, GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last)),
                                  BRep_Tool::Tolerance(edge));
      }
    }
  }
  return body;
}

// The ridge's two pieces, from the roof's ends to the rod.
std::vector<TopoDS_Edge> ridge_of(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> ridge;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (curve.GetType() == GeomAbs_Line && std::abs(middle.X()) < 1e-9 && std::abs(middle.Z() - kRidge) < 1e-9) {
      ridge.push_back(TopoDS::Edge(edges(i)));
    }
  }
  return ridge;
}

// The material the rounding takes off the ridge: its cross-section (between
// the two roof planes and the blend's arc) along the whole ridge, less what
// lies in the rod.
double removed_by_the_rounding() {
  const double slope = std::atan2(kRidge, kHalf); // each roof plane's angle
  const double delta = 2.0 * slope;                // the angle between the planes' normals
  // The blend's axis lies under the ridge, kRound from both planes; its
  // contacts lie on the planes, kRound * tan(delta / 2) from the ridge.
  const double d = kRound * std::tan(0.5 * delta);
  const double axis_z = kRidge - kRound / std::cos(0.5 * delta);
  const gp_Pnt ridge(0, -kHalf, kRidge);
  const gp_Pnt left(-d * std::cos(slope), -kHalf, kRidge - d * std::sin(slope));
  const gp_Pnt right(d * std::cos(slope), -kHalf, kRidge - d * std::sin(slope));
  const gp_Pnt top(0, -kHalf, axis_z + kRound);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(left, ridge).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(ridge, right).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(right, top, left).Value()).Edge());
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape strip = BRepPrimAPI_MakePrism(section, gp_Vec(0, 2.0 * kHalf, 0)).Shape();
  return volume_of(BRepAlgoAPI_Cut(strip, rod(0.5 * kPi)).Shape());
}

void round_the_ridge(double seam) {
  const TopoDS_Shape body = roof_with_a_rod(seam);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> ridge = ridge_of(body);
  CHECK(ridge.size() == 2);
  if (ridge.size() != 2) {
    return;
  }
  TopoDS_Shape rounded;
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    for (const TopoDS_Edge& edge : ridge) {
      fillet.Add(kRound, edge);
    }
    fillet.Build();
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      return;
    }
    rounded = fillet.Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    CHECK(false);
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  CHECK_NEAR_TOL(volume_of(body) - volume_of(rounded), removed_by_the_rounding(), 1e-6);
}

// The seam through the ridge's end at +y, and at -y.
void test_rounding_ending_where_the_seam_ends_at_plus_y() { round_the_ridge(0.5 * kPi); }

void test_rounding_ending_where_the_seam_ends_at_minus_y() { round_the_ridge(1.5 * kPi); }

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_seam_run_on) {
  test::guarded("test_rounding_ending_where_the_seam_ends_at_plus_y",
                test::test_rounding_ending_where_the_seam_ends_at_plus_y);
  test::guarded("test_rounding_ending_where_the_seam_ends_at_minus_y",
                test::test_rounding_ending_where_the_seam_ends_at_minus_y);
  if (test::failures == 0) {
    std::puts("test_fillet_seam_run_on: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
