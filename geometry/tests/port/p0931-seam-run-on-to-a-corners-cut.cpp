// SPDX-License-Identifier: MIT
// Port patch 0931: a corner whose cut on the face at the end crosses the
// line of that face's seam past the seam's end, at the corner's vertex.
// PerformOneCorner split the cut where it crosses the seam only where the
// crossing lies on the seam's edge; past the seam's end the cut was left
// whole across the line, the seam ended at the vertex the rounding removes,
// and the face at the end came out with a wire that is not closed. The
// patch splits the cut where it crosses the seam's line and runs the seam
// on along the line to the cut.
//
// The body: a roof (eaves at z = 0, 20 by 20, its ridge along y at z = 5)
// with a rod of radius 2.5 standing through it on the z axis, the rod's
// seam on the line through the point where the ridge meets it. The ridge's
// two pieces are rounded with 1. The rounding takes material off the
// ridge, so the rod's face grows down to the rounding's cut, which crosses
// the seam's line below the vertex where the seam ended. The seam's curve
// is made a B-spline, as a stored body's is: a line seam is prolonged past
// its edge by OCCT's own way, a B-spline curve is not. Without the patch
// the result is invalid and its volume wrong (about 213 mm3 instead of
// 0.5456 removed); with it the result is valid and the ridge loses the
// blend's section along it less the part inside the rod. The seam on the
// line through the ridge's other end is the same case; a seam away from
// the ridge (at +x), and the seam left a line, build the same with and
// without the patch.

#include <cmath>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kHalf  = 10.0; // the roof's half width (x) and half length (y)
constexpr double kRidge = 5.0;  // the ridge's height over the eaves
constexpr double kRod   = 2.5;  // the rod's radius
constexpr double kRound = 1.0;  // the ridge's rounding

// The rod on the z axis, its seam at the angle `seam` from +x.
TopoDS_Shape rod(double seam) {
  return BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ(), gp_Dir(std::cos(seam), std::sin(seam), 0)), kRod,
                                  3.0 * kRidge)
    .Shape();
}

// The roof and the rod joined; with `bspline` the seam's line becomes a
// B-spline curve.
TopoDS_Shape roof_with_a_rod(double seam, bool bspline) {
  BRepBuilderAPI_MakePolygon section(gp_Pnt(-kHalf, -kHalf, 0), gp_Pnt(kHalf, -kHalf, 0), gp_Pnt(0, -kHalf, kRidge),
                                     true);
  const TopoDS_Shape roof =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(section.Wire(), true).Face(), gp_Vec(0, 2.0 * kHalf, 0)).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Fuse(roof, rod(seam)).Shape();
  if (!bspline) {
    return body;
  }
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge&            edge  = TopoDS::Edge(e.Current());
      double                        first = 0.0, last = 0.0;
      const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
      if (BRep_Tool::IsClosed(edge, face) && !curve.IsNull() && !curve->IsKind(STANDARD_TYPE(Geom_BSplineCurve))) {
        BRep_Builder().UpdateEdge(edge, GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last)),
                                  BRep_Tool::Tolerance(edge));
      }
    }
  }
  return body;
}

// The material the rounding takes off the ridge: its section (between the
// two roof planes and the blend's arc) along the whole ridge, less what
// lies in the rod.
double removed_by_the_rounding() {
  const double slope  = std::atan2(kRidge, kHalf); // each roof plane's angle
  const double delta  = 2.0 * slope;               // the angle between the planes' normals
  const double d      = kRound * std::tan(0.5 * delta);
  const double axis_z = kRidge - kRound / std::cos(0.5 * delta);
  const gp_Pnt ridge(0, -kHalf, kRidge);
  const gp_Pnt left(-d * std::cos(slope), -kHalf, kRidge - d * std::sin(slope));
  const gp_Pnt right(d * std::cos(slope), -kHalf, kRidge - d * std::sin(slope));
  const gp_Pnt top(0, -kHalf, axis_z + kRound);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(left, ridge).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(ridge, right).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(right, top, left).Value()).Edge());
  const TopoDS_Shape strip =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(0, 2.0 * kHalf, 0)).Shape();
  return port::volume_of(BRepAlgoAPI_Cut(strip, rod(0.0)).Shape());
}

void round_the_ridge(double seam, bool bspline) {
  const TopoDS_Shape body = roof_with_a_rod(seam, bspline);
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> ridge = port::edges_where(
    body, [](const gp_Pnt& p) { return std::abs(p.X()) < 1e-9 && std::abs(p.Z() - kRidge) < 1e-9; });
  CHECK(ridge.size() == 2u);
  if (ridge.size() != 2u) {
    return;
  }
  const TopoDS_Shape rounded = port::rounded(body, ridge, kRound);
  CHECK(port::valid(rounded));
  if (rounded.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(body) - port::volume_of(rounded), removed_by_the_rounding(), 1e-6);
}

// The seam through the ridge's end at +y, and at -y.
void test_rounding_ending_where_the_rods_seam_ends_at_plus_y() { round_the_ridge(0.5 * test::kPi, true); }
void test_rounding_ending_where_the_rods_seam_ends_at_minus_y() { round_the_ridge(1.5 * test::kPi, true); }

// The seam away from the ridge, and the seam a line.
void test_rounding_with_the_rods_seam_away_from_the_ridge() { round_the_ridge(0.0, true); }
void test_rounding_ending_where_the_rods_line_seam_ends() { round_the_ridge(0.5 * test::kPi, false); }

} // namespace

MITCAD_PORT_TEST("0931", test_rounding_ending_where_the_rods_seam_ends_at_plus_y);
MITCAD_PORT_TEST("0931", test_rounding_ending_where_the_rods_seam_ends_at_minus_y);
MITCAD_PORT_TEST("0931", test_rounding_with_the_rods_seam_away_from_the_ridge);
MITCAD_PORT_TEST("0931", test_rounding_ending_where_the_rods_line_seam_ends);
