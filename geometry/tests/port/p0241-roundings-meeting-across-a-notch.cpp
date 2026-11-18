// SPDX-License-Identifier: MIT
// Port patch 0241: two roundings that meet across a small notch in the rim
// they round, the notch's edges not rounded. OCCT ran each end's contact on
// the top on to the end of its domain on that face and the corner did not
// cut it back: the two contacts overlapped ("fillets have too big
// radiuses"). With the patch one end's blend runs on over the notch, cut by
// the notch's faces, to the other end's section, and the two blends are one
// torus face.
//
// test_fillet_results.cpp rounds a disc's outer rim (convex wall) across a
// notch. Here the rim of a half-round bay of radius 10 cut into a plate's
// side (a concave wall, the material outside the cylinder), broken at its
// deepest point by a notch 0.1 deep into the plate and 0.34 wide at the
// wall; both arcs rounded with 0.3, whose contact on the top (0.3 from the
// wall) passes above the notch: built, valid, one torus face across the
// notch, the same body as the whole rim rounded and the notch cut after.
// The boundary: a notch 1 deep (deeper than the ball reaches), the two
// roundings end at the notch's sides as stock: built, valid, two torus
// faces, the same body as the whole rim rounded and the notch cut after.

#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

// The edge of a shape whose middle lies nearest to p.
TopoDS_Edge nearest_edge(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double      nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&            candidate = TopoDS::Edge(e.Current());
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(candidate, first, last);
    if (curve.IsNull()) {
      continue;
    }
    const double d = curve->Value(0.5 * (first + last)).Distance(p);
    if (d < nearest) {
      nearest = d;
      best    = candidate;
    }
  }
  return best;
}

int torus_faces(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    n += BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Torus ? 1 : 0;
  }
  return n;
}

// A plate x in [-20, 20], y in [-20, 0], z in [0, 5] with a half-round bay
// of radius 10 about the z axis cut into its side y = 0 (the cylinder's
// seam outside the plate): the bay's rim on the top one arc from
// (-10, 0, 5) round through (0, -10, 5) to (10, 0, 5).
TopoDS_Shape plate_with_bay() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, 0), gp_Pnt(20, 0, 5)).Shape();
  const TopoDS_Shape bay =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), 10, 7).Shape();
  return BRepAlgoAPI_Cut(plate, bay).Shape();
}

// A notch through the plate's whole height at the bay's deepest point,
// `depth` deep into the plate: a trapezoid 0.6 wide 0.6 inside the bay,
// 0.3 wide at its bottom (0.34 wide at the wall for a depth of 0.1, 0.49
// for 1).
TopoDS_Shape notch(double depth) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& [x, y] :
       std::vector<std::pair<double, double>>{{0.3, -9.4}, {0.15, -10 - depth}, {-0.15, -10 - depth}, {-0.3, -9.4}}) {
    polygon.Add(gp_Pnt(x, y, -1));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 7)).Shape();
}

// The point of the bay's rim at the angle (degrees from +x), on the top.
gp_Pnt on_rim(double degrees) {
  const double a = degrees * test::kPi / 180.0;
  return gp_Pnt(10.0 * std::cos(a), 10.0 * std::sin(a), 5.0);
}

// The notched bay's two arcs rounded with 0.3, and the whole rim of the bay
// without the notch rounded with 0.3 and the notch cut after.
struct Rounded {
  TopoDS_Shape both;
  TopoDS_Shape expected;
};

Rounded rounded_bay(double depth) {
  const TopoDS_Shape bay  = plate_with_bay();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(bay, notch(depth)).Shape();
  CHECK(port::valid(body));
  Rounded            r;
  const TopoDS_Shape whole = port::rounded(bay, {nearest_edge(bay, on_rim(-90))}, 0.3);
  CHECK(port::valid(whole));
  if (port::valid(whole)) {
    r.expected = BRepAlgoAPI_Cut(whole, notch(depth)).Shape();
  }
  r.both = port::rounded(body, {nearest_edge(body, on_rim(-135)), nearest_edge(body, on_rim(-45))}, 0.3);
  return r;
}

// The notch 0.1 deep: the contact on the top passes above it, the two
// blends meet over it as one torus face.
void test_bay_rim_rounded_across_a_notch() {
  const Rounded r = rounded_bay(0.1);
  CHECK(port::valid(r.both));
  if (!port::valid(r.both) || r.expected.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(r.both), port::volume_of(r.expected), 1e-7);
  CHECK(port::count_of(r.both, TopAbs_FACE) == port::count_of(r.expected, TopAbs_FACE));
  CHECK(torus_faces(r.both) == 1);
  CHECK(torus_faces(r.expected) == 1);
}

// The boundary: the notch 1 deep, deeper than the ball reaches; the
// roundings end at its sides, two torus faces, as stock.
void test_bay_rim_rounded_up_to_a_deep_notch() {
  const Rounded r = rounded_bay(1.0);
  CHECK(port::valid(r.both));
  if (!port::valid(r.both) || r.expected.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(r.both), port::volume_of(r.expected), 1e-6);
  CHECK(torus_faces(r.both) == 2);
}

} // namespace

MITCAD_PORT_TEST("0241", test_bay_rim_rounded_across_a_notch);
MITCAD_PORT_TEST("0241", test_bay_rim_rounded_up_to_a_deep_notch);
