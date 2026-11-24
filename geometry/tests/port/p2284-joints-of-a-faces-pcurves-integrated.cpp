// SPDX-License-Identifier: MIT
// Port patch 2284: the volume integration (BRepGProp_Gauss) integrates
// across the joints of each face's loop of pcurves, and from each edge's
// curve on the face to its 3D curve, so a shell closed in space has one
// volume about every point it is integrated about (mitcad#2284).
//
// The body, built from scratch: half a cylinder of radius 10 and height 10
// (its side face from u = 0 to pi, v the height), one of whose edges'
// pcurves on the side face is changed by 1e-6 mm, the edge's and its
// vertices' tolerances raised to 1e-5 mm to cover it. Its volume is
// integrated about its middle and about points 100 mm away from it along
// X, Y and Z; a closed shell gives one volume about all of them, pi R^2 H
// / 2.
//
// - The side line at u = pi, its pcurve ending 1e-6 short of the top
//   (v from 0 to H - 1e-6 along the line's 0 to H): its pcurve lies on its
//   3D line, but the loop of pcurves meets the top arc's pcurve 1e-6 apart
//   along v. An integration along the pcurves alone left out the strip
//   from u = 0 to pi, 1e-6 wide, below the arc; its normal integrated over
//   it is 2e-5 mm2 along Y, so the volume moved by 6.7e-4 mm3 (4.2e-7
//   relative) with the point 100 mm along Y. With the joint integrated,
//   one volume.
// - The top arc, its pcurve moved 1e-6 up along v: the loop of pcurves
//   closed at its joints reaches 1e-6 above the top plane, which is bounded
//   by the arc's 3D curve. The ribbon from the arc's curve on the side face
//   to its 3D curve takes that strip away again: one volume, the half
//   cylinder's (an integration of the loop closed at its joints alone
//   would move by 4.2e-7 relative; one along the pcurves alone happened to
//   give the half cylinder's, the arc running along u).
// - The half cylinder as built, every pcurve on its 3D curve: its volume
//   as before.

#include <algorithm>
#include <cmath>

#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp_Domain.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepGProp_Vinert.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <NCollection_Array1.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 10.0;
constexpr double kHeight = 10.0;
// How far the changed pcurve moves.
constexpr double kShift = 1e-6;
// The tolerance of the changed edge and of its vertices.
constexpr double kLoose = 1e-5;

// The body's volume about `point` (BRepGProp_Vinert, each face to a
// relative error of 1e-9).
double volume_about(const TopoDS_Shape& shape, const gp_Pnt& point) {
  double volume = 0.0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    BRepGProp_Vinert integral;
    integral.SetLocation(point);
    BRepGProp_Face surface(face);
    BRepGProp_Domain domain(face);
    integral.Perform(surface, domain, 1e-9);
    volume += integral.Mass();
  }
  return volume;
}

TopoDS_Shape half_cylinder() {
  return BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), kRadius, kHeight, test::kPi)
      .Shape();
}

double exact_volume() { return 0.5 * test::kPi * kRadius * kRadius * kHeight; }

// The side face (the cylindrical one) of a half cylinder.
TopoDS_Face side_face(const TopoDS_Shape& body) {
  for (TopExp_Explorer faces(body, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face& face = TopoDS::Face(faces.Current());
    if (BRepAdaptor_Surface(face, false).GetType() == GeomAbs_Cylinder) {
      return face;
    }
  }
  return TopoDS_Face();
}

// Gives `edge` the pcurve `pcurve` on `face` and raises the edge's and its
// vertices' tolerances to kLoose.
void replace_pcurve(const TopoDS_Edge& edge, const TopoDS_Face& face, const occ::handle<Geom2d_Curve>& pcurve) {
  BRep_Builder builder;
  builder.UpdateEdge(edge, pcurve, face, kLoose);
  TopoDS_Vertex from;
  TopoDS_Vertex to;
  TopExp::Vertices(edge, from, to);
  builder.UpdateVertex(from, kLoose);
  builder.UpdateVertex(to, kLoose);
}

// Ends the side face's pcurve of its line at u = pi (the largest u) kShift
// short of the top: a straight pcurve from the line's bottom to (pi, H -
// kShift) over the line's parameters. Whether the line was found.
bool shorten_the_side_line(const TopoDS_Shape& body) {
  const TopoDS_Face face = side_face(body);
  if (face.IsNull()) {
    return false;
  }
  for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull()) {
      continue;
    }
    gp_Pnt2d at_first = pcurve->Value(first);
    gp_Pnt2d at_last = pcurve->Value(last);
    if (std::abs(at_first.X() - test::kPi) > 1e-9 || std::abs(at_last.X() - test::kPi) > 1e-9) {
      continue;
    }
    gp_Pnt2d& top = at_first.Y() > at_last.Y() ? at_first : at_last;
    if (std::abs(top.Y() - kHeight) > 1e-9) {
      continue;
    }
    top.SetY(kHeight - kShift);
    NCollection_Array1<gp_Pnt2d> poles(1, 2);
    poles(1) = at_first;
    poles(2) = at_last;
    NCollection_Array1<double> knots(1, 2);
    knots(1) = first;
    knots(2) = last;
    NCollection_Array1<int> mults(1, 2);
    mults(1) = 2;
    mults(2) = 2;
    replace_pcurve(edge, face, new Geom2d_BSplineCurve(poles, knots, mults, 1));
    return true;
  }
  return false;
}

// Moves the side face's pcurve of its top arc kShift up along v. Whether
// the arc was found.
bool lift_the_top_arc(const TopoDS_Shape& body) {
  const TopoDS_Face face = side_face(body);
  if (face.IsNull()) {
    return false;
  }
  for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull() || BRep_Tool::IsClosed(edge, face)) {
      continue;
    }
    const gp_Pnt2d start = pcurve->Value(first);
    const gp_Pnt2d end = pcurve->Value(last);
    if (std::abs(start.Y() - kHeight) > 1e-9 || std::abs(end.Y() - kHeight) > 1e-9 ||
        std::abs(start.X() - end.X()) < 1.0) {
      continue;
    }
    const occ::handle<Geom2d_Curve> moved = occ::down_cast<Geom2d_Curve>(pcurve->Copy());
    moved->Translate(gp_Vec2d(0.0, kShift));
    replace_pcurve(edge, face, moved);
    return true;
  }
  return false;
}

// The volumes about the middle and about points 100 mm away from it along
// X, Y and Z; the largest difference from the first, relative.
double spread_about_points(const TopoDS_Shape& body, double& about_middle) {
  const gp_Pnt middle(0.0, 4.0, 5.0);
  about_middle = volume_about(body, middle);
  double spread = 0.0;
  for (const gp_Vec& away : {gp_Vec(100.0, 0.0, 0.0), gp_Vec(0.0, 100.0, 0.0), gp_Vec(0.0, 0.0, 100.0)}) {
    const double volume = volume_about(body, middle.Translated(away));
    spread = std::max(spread, std::abs(volume - about_middle) / std::abs(about_middle));
  }
  return spread;
}

void check_one_volume(const TopoDS_Shape& body, const char* what) {
  double volume = 0.0;
  const double spread = spread_about_points(body, volume);
  std::printf("  %s: volume %.9f (exact %.9f), spread about the points %.3g\n", what, volume, exact_volume(),
              spread);
  CHECK(spread <= 1e-9);
  CHECK(std::abs(volume - exact_volume()) <= 1e-9 * exact_volume());
  CHECK(std::abs(port::volume_of(body) - exact_volume()) <= 1e-9 * exact_volume());
}

// The side line's pcurve 1e-6 short of the top arc's: the joint integrated.
void test_joint_apart_along_v_integrated() {
  const TopoDS_Shape body = half_cylinder();
  CHECK(shorten_the_side_line(body));
  CHECK(port::valid(body));
  check_one_volume(body, "the side line's pcurve 1e-6 short of the top");
}

// The top arc's pcurve 1e-6 above its 3D curve: the ribbon between them
// integrated.
void test_pcurve_off_its_edge_closed_in_space() {
  const TopoDS_Shape body = half_cylinder();
  CHECK(lift_the_top_arc(body));
  CHECK(port::valid(body));
  check_one_volume(body, "the top arc's pcurve 1e-6 above its 3D curve");
}

// The half cylinder as built, its loops closed: its volume as before.
void test_closed_loops_unchanged() { check_one_volume(half_cylinder(), "the half cylinder as built"); }

} // namespace

MITCAD_PORT_TEST("2284", test_joint_apart_along_v_integrated);
MITCAD_PORT_TEST("2284", test_pcurve_off_its_edge_closed_in_space);
MITCAD_PORT_TEST("2284", test_closed_loops_unchanged);
