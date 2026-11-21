// SPDX-License-Identifier: MIT
// Port patch 0910: an OnSame end's cap starts at the opposite contact's
// crossing with the prolonged edge itself, not at the point where the
// contact's approximated curve meets the face at the end, a hair off that
// edge.
//
// A pin of radius 3 stands out of a wedge-shaped top face whose two sides
// are tangent to the pin, 1.1 rad from +x; the pin's foot on the top is
// rounded with 0.3 or 0.5. At each end the blend runs on to a side (the
// face at the end), which goes on tangentially from the pin: the blend is
// nearly tangent to the side there, so the cap, the blend's intersection
// with the side, rises from the side's edge with the pin slowly, and where
// it starts off that edge it runs off the exact section. Before the patch
// the material added came out 3.5e-5 and 6.6e-5 off the exact ring (the
// cap from the contact curve's point on the side); with it, within 6.1e-6.
// The same pin with its sides 0.7 rad from +x and rounded with 0.5 is the
// control (exact within 1e-7 with or without the patch). The original case,
// a thread's chamfer cone whose cap crossed the root cylinder's circle and
// left its wire self-intersecting, hangs on a contact curve 1.3e-6 off the
// cone; no synthetic body reproduced that crossing.

#include <cmath>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>

#include "port_test.hpp"

namespace {

constexpr double kPin = 3.0;  // the pin's radius
constexpr double kTop = 10.0; // the top face's height

// The wedge's outline at height z: from the tangent points T1 = (r cos a,
// r sin a) and T2 = (r cos a, -r sin a) along the tangents away from +x,
// closed through the pin by the chord between them.
TopoDS_Face wedge_outline(double angle, double z) {
  const double c = std::cos(angle), s = std::sin(angle), far = 40.0;
  const gp_Pnt t1(kPin * c, kPin * s, z), t2(kPin * c, -kPin * s, z);
  BRepBuilderAPI_MakePolygon polygon(t1, gp_Pnt(t1.X() - far * s, t1.Y() + far * c, z),
                                     gp_Pnt(t2.X() - far * s, t2.Y() - far * c, z), t2, true);
  return BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
}

// A block (z 0 to 7) under the wedge (z 7 to 10) and the pin (z 7 to 12)
// standing out of it, its seam at T1.
TopoDS_Shape pin_on_a_wedge(double angle) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-30, -45, 0), gp_Pnt(10, 45, 7)).Shape();
  const TopoDS_Shape wedge = BRepPrimAPI_MakePrism(wedge_outline(angle, 7.0), gp_Vec(0, 0, kTop - 7.0)).Shape();
  const TopoDS_Shape pin =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 7), gp_Dir(0, 0, 1), gp_Dir(std::cos(angle), std::sin(angle), 0)),
                             kPin, 5.0)
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(block, wedge).Shape(), pin).Shape(), true,
                                     true, true);
  unify.Build();
  return unify.Shape();
}

// The pin's foot on the top face: the circle edge at z = kTop.
std::vector<TopoDS_Edge> foot_of(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> foot;
  for (const TopoDS_Edge& edge : port::edges_where(body, [](const gp_Pnt& p) { return std::abs(p.Z() - kTop) < 1e-9; })) {
    BRepAdaptor_Curve curve(edge);
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - kPin) < 1e-9) {
      foot.push_back(edge);
    }
  }
  return foot;
}

// The material a rounding of radius r adds at the pin's foot all round,
// within the wedge's sides (their planes cut it).
double foot_ring_in_the_wedge(double angle, double r) {
  // The section: the square at the corner less the ball's quarter.
  const gp_Pnt c0(kPin, 0, kTop);
  BRepBuilderAPI_MakePolygon square(c0, gp_Pnt(kPin + r, 0, kTop), gp_Pnt(kPin + r, 0, kTop + r),
                                    gp_Pnt(kPin, 0, kTop + r), true);
  const TopoDS_Face  corner = BRepBuilderAPI_MakeFace(square.Wire(), true).Face();
  const TopoDS_Shape ball =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kPin + r, -1, kTop + r), gp_Dir(0, 1, 0)), r, 2.0).Shape();
  const TopoDS_Shape section = BRepAlgoAPI_Cut(corner, ball).Shape();
  const TopoDS_Shape ring    = BRepPrimAPI_MakeRevol(section, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape();
  const TopoDS_Shape wedge   = BRepPrimAPI_MakePrism(wedge_outline(angle, kTop - 1.0), gp_Vec(0, 0, 3.0)).Shape();
  return port::volume_of(BRepAlgoAPI_Common(ring, wedge).Shape());
}

// The foot rounded with r: valid, one torus, the material added the exact
// ring's within `relative`.
void check_foot(double angle, double r, double relative) {
  std::printf("sides %.2f rad from +x, rounded with %.1f\n", angle, r);
  const TopoDS_Shape body = pin_on_a_wedge(angle);
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> foot = foot_of(body);
  CHECK(foot.size() == 1);
  if (foot.size() != 1) {
    return;
  }
  const TopoDS_Shape rounded = port::rounded(body, foot, r);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  int tori = 0;
  for (TopExp_Explorer f(rounded, TopAbs_FACE); f.More(); f.Next()) {
    tori += BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Torus ? 1 : 0;
  }
  CHECK(tori == 1);
  const double added = port::volume_of(rounded) - port::volume_of(body);
  const double exact = foot_ring_in_the_wedge(angle, r);
  std::printf("  added %.9f, exact %.9f, relative %.2e\n", added, exact, (added - exact) / exact);
  // (Relative to the ring itself, which is smaller than 1.)
  CHECK(std::abs(added - exact) <= relative * exact);
}

void test_caps_start_on_the_sides_edges_with_the_pin() {
  check_foot(1.1, 0.3, 1.5e-5);
  check_foot(1.1, 0.5, 1.5e-5);
}

// The control: sides further round the pin, the caps exact either way.
void test_caps_of_a_wider_wedge() {
  check_foot(0.7, 0.5, 1e-6);
}

} // namespace

MITCAD_PORT_TEST("0910", test_caps_start_on_the_sides_edges_with_the_pin);
MITCAD_PORT_TEST("0910", test_caps_of_a_wider_wedge);
