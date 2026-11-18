// SPDX-License-Identifier: MIT
// Port patch 1210: a rounding over the mouth of a slot whose rounded end
// touches the top face along a crest line (BRepFilletAPI_MakeFillet::
// BuildOverSlotMouths). A second geometry beside the rim of eight arcs of
// geometry/tests/test_fillet_slot_mouths.cpp: a straight wall, its top edge
// broken by one fin.
//
// A block 20 x 10 x 6 (x from -10 to 10, y from 0 to 10, z from 0 to 6);
// out of its front wall y = 0 stands a fin 1 thick (x from -0.5 to 0.5,
// y from -3 to 0) from the bottom up, its top a cylinder C of radius 0.5
// about the axis x = 0, z = 5.5 along y, so that C's crest touches the top
// plane's height along the line x = 0, z = 6 and meets the block's top at
// the vertex V = (0, 0, 6) with no edge along it. The front wall is two
// faces A and B (x < 0 and x > 0); four edges meet at V: the top's front
// edges with A and with B (tangent, one line) and the curves where C meets
// A and B. OCCT's booleans cannot make the crest touching the top, so the
// body is sewn from faces made one by one.
//
// Both front edges rounded with 1.5 (larger than C's radius): without the
// patch the corner at V fails (no edge between the top and C round the
// vertex) and the fillet fails. With it: built, valid, one solid; the
// blend runs along the whole front, and within the fin's width the fin's
// profile (its walls and C, prolonged into the block) grows up to the
// blend, C's crest up to the blend's contact on the top at (0, 1.5, 6). The
// material: the block's less the rounding's section along the 20 of the
// front, plus the part of that section inside the fin's profile over the
// fin's width.
//
// The boundary: the same fin with C's crest 0.01 below the top (no crest
// on the edge, the front one face, its top edge one edge, made with OCCT's
// booleans): not a mouth, the fillet as OCCT's; where it is built, valid.

#include <cmath>
#include <cstdio>
#include <initializer_list>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>

#include "port_test.hpp"

namespace {

constexpr double kW     = 10.0; // the block's half width (x)
constexpr double kD     = 10.0; // its depth (y)
constexpr double kH     = 6.0;  // its height (z)
constexpr double kRho   = 0.5;  // the fin's half width, C's radius
constexpr double kFin   = 3.0;  // the fin's length out of the wall
constexpr double kRound = 1.5;  // the rounding's radius

// A face bounded by the polygon through the points.
TopoDS_Face polygon_face(std::initializer_list<gp_Pnt> points) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt& p : points) {
    polygon.Add(p);
  }
  polygon.Close();
  return BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
}

TopoDS_Edge line(const gp_Pnt& a, const gp_Pnt& b) { return BRepBuilderAPI_MakeEdge(a, b).Edge(); }

TopoDS_Edge arc(const gp_Pnt& a, const gp_Pnt& m, const gp_Pnt& b) {
  const occ::handle<Geom_TrimmedCurve> curve = GC_MakeArcOfCircle(a, m, b).Value();
  return BRepBuilderAPI_MakeEdge(curve).Edge();
}

// A planar face bounded by the edges in turn.
TopoDS_Face planar_face(std::initializer_list<TopoDS_Edge> edges) {
  BRepBuilderAPI_MakeWire wire;
  for (const TopoDS_Edge& edge : edges) {
    wire.Add(edge);
  }
  return BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
}

// The point of the fin's profile (rectangle up to the axis height a,
// semicircle of radius kRho above it) at angle t from the top, at y.
gp_Pnt on_top(double t, double y, double a) { return gp_Pnt(-kRho * std::sin(t), y, a + kRho * std::cos(t)); }

// The block with the fin whose crest touches the top's height at V: sewn
// from its faces. The top's front edge is split at V; C is one face, its
// edge on the front cut at V by the sewing (the walls' arcs end there).
TopoDS_Shape mouth_body() {
  const double  a = kH - kRho; // C's axis height
  const double  q = 0.25 * test::kPi;
  const double  h = kRho, L = kFin;
  const gp_Pnt  v(0, 0, kH);
  BRepBuilderAPI_Sewing sewing(1e-6);
  // The top, its front edge split at V.
  sewing.Add(polygon_face({gp_Pnt(-kW, 0, kH), v, gp_Pnt(kW, 0, kH), gp_Pnt(kW, kD, kH), gp_Pnt(-kW, kD, kH)}));
  // The front walls A (x < 0) and B (x > 0).
  sewing.Add(planar_face({line(gp_Pnt(-kW, 0, 0), gp_Pnt(-h, 0, 0)), line(gp_Pnt(-h, 0, 0), gp_Pnt(-h, 0, a)),
                          arc(gp_Pnt(-h, 0, a), on_top(q, 0, a), v), line(v, gp_Pnt(-kW, 0, kH)),
                          line(gp_Pnt(-kW, 0, kH), gp_Pnt(-kW, 0, 0))}));
  sewing.Add(planar_face({line(gp_Pnt(h, 0, 0), gp_Pnt(kW, 0, 0)), line(gp_Pnt(kW, 0, 0), gp_Pnt(kW, 0, kH)),
                          line(gp_Pnt(kW, 0, kH), v), arc(v, on_top(-q, 0, a), gp_Pnt(h, 0, a)),
                          line(gp_Pnt(h, 0, a), gp_Pnt(h, 0, 0))}));
  // The block's sides, back and the bottom (with the fin's).
  sewing.Add(polygon_face({gp_Pnt(-kW, 0, 0), gp_Pnt(-kW, kD, 0), gp_Pnt(-kW, kD, kH), gp_Pnt(-kW, 0, kH)}));
  sewing.Add(polygon_face({gp_Pnt(kW, 0, 0), gp_Pnt(kW, 0, kH), gp_Pnt(kW, kD, kH), gp_Pnt(kW, kD, 0)}));
  sewing.Add(polygon_face({gp_Pnt(-kW, kD, 0), gp_Pnt(kW, kD, 0), gp_Pnt(kW, kD, kH), gp_Pnt(-kW, kD, kH)}));
  sewing.Add(polygon_face({gp_Pnt(-kW, 0, 0), gp_Pnt(-h, 0, 0), gp_Pnt(-h, -L, 0), gp_Pnt(h, -L, 0),
                           gp_Pnt(h, 0, 0), gp_Pnt(kW, 0, 0), gp_Pnt(kW, kD, 0), gp_Pnt(-kW, kD, 0)}));
  // The fin's walls, its end and its top C (seam at its bottom, out of
  // the face: no edge along the crest).
  sewing.Add(polygon_face({gp_Pnt(-h, 0, 0), gp_Pnt(-h, -L, 0), gp_Pnt(-h, -L, a), gp_Pnt(-h, 0, a)}));
  sewing.Add(polygon_face({gp_Pnt(h, 0, 0), gp_Pnt(h, 0, a), gp_Pnt(h, -L, a), gp_Pnt(h, -L, 0)}));
  sewing.Add(planar_face({line(gp_Pnt(-h, -L, 0), gp_Pnt(h, -L, 0)), line(gp_Pnt(h, -L, 0), gp_Pnt(h, -L, a)),
                          arc(gp_Pnt(h, -L, a), gp_Pnt(0, -L, kH), gp_Pnt(-h, -L, a)),
                          line(gp_Pnt(-h, -L, a), gp_Pnt(-h, -L, 0))}));
  const occ::handle<Geom_CylindricalSurface> top =
    new Geom_CylindricalSurface(gp_Ax3(gp_Pnt(0, -L, a), gp_Dir(0, 1, 0), gp_Dir(0, 0, -1)), h);
  sewing.Add(BRepBuilderAPI_MakeFace(top, 0.5 * test::kPi, 1.5 * test::kPi, 0.0, L, Precision::Confusion()).Face());
  sewing.Perform();
  const TopoDS_Shape sewn = sewing.SewedShape();
  if (sewn.IsNull() || sewn.ShapeType() != TopAbs_SHELL) {
    std::fprintf(stderr, "p1210: the sewn faces are not one shell\n");
    return TopoDS_Shape();
  }
  TopoDS_Solid solid = BRepBuilderAPI_MakeSolid(TopoDS::Shell(sewn)).Solid();
  BRepLib::OrientClosedSolid(solid);
  return solid;
}

// The same block and fin with C's crest `below` under the top, made with
// OCCT's booleans (the fin a prism of its profile).
TopoDS_Shape fin_below_top_body(double below) {
  const double a = kH - kRho - below;
  const double h = kRho, L = kFin;
  const TopoDS_Face profile =
    planar_face({line(gp_Pnt(-h, -L, 0), gp_Pnt(h, -L, 0)), line(gp_Pnt(h, -L, 0), gp_Pnt(h, -L, a)),
                 arc(gp_Pnt(h, -L, a), gp_Pnt(0, -L, a + kRho), gp_Pnt(-h, -L, a)),
                 line(gp_Pnt(-h, -L, a), gp_Pnt(-h, -L, 0))});
  const TopoDS_Shape fin   = BRepPrimAPI_MakePrism(profile, gp_Vec(0, L, 0)).Shape();
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-kW, 0, 0), gp_Pnt(kW, kD, kH)).Shape();
  return BRepAlgoAPI_Fuse(block, fin).Shape();
}

// The number of edges at the vertex of the shape at p (-1 where none is).
int edges_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ve;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_VERTEX, TopAbs_EDGE, ve);
  for (int i = 1; i <= ve.Extent(); ++i) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(ve.FindKey(i))).Distance(p) < 1e-6) {
      return ve(i).Extent();
    }
  }
  return -1;
}

bool has_vertex_near(const TopoDS_Shape& shape, const gp_Pnt& p, double tol) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p) <= tol) {
      return true;
    }
  }
  return false;
}

// The block's front edges at the top (y = 0, z = kH).
std::vector<TopoDS_Edge> front_top_edges(const TopoDS_Shape& body) {
  return port::edges_where(body, [](const gp_Pnt& p) { return std::abs(p.Y()) < 1e-6 && std::abs(p.Z() - kH) < 1e-6; });
}

// The part of the rounding's section (the corner between the top and the
// wall outside the ball's arc) at x that lies inside the fin's profile
// prolonged: up to C's height there.
double section_inside_fin(double x) {
  const double r = kRound;
  const double t = r - kRho + std::sqrt(std::max(0.0, kRho * kRho - x * x)); // above the ball's centre
  return r * t - 0.5 * t * std::sqrt(r * r - t * t) - 0.5 * r * r * std::asin(t / r);
}

// That part integrated over the fin's width (x = kRho sin s, Simpson's rule).
double kept_within_fin() {
  const int n   = 2000;
  double    sum = 0.0;
  for (int i = 0; i <= n; ++i) {
    const double s = -0.5 * test::kPi + test::kPi * i / n;
    const double w = (i == 0 || i == n) ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0);
    sum += w * section_inside_fin(kRho * std::sin(s)) * kRho * std::cos(s);
  }
  return sum * test::kPi / n / 3.0;
}

// Both front edges rounded with 1.5 over the fin's mouth.
void test_rounding_over_a_slot_mouth_in_a_straight_wall() {
  const TopoDS_Shape body = mouth_body();
  CHECK(port::valid(body));
  if (!port::valid(body)) {
    return;
  }
  const double a = kH - kRho;
  CHECK_NEAR(port::volume_of(body), 2.0 * kW * kD * kH + kFin * (2.0 * kRho * a + 0.5 * test::kPi * kRho * kRho));
  CHECK(edges_at(body, gp_Pnt(0, 0, kH)) == 4);
  const std::vector<TopoDS_Edge> front = front_top_edges(body);
  CHECK(front.size() == 2);
  if (front.size() != 2) {
    return;
  }
  const TopoDS_Shape result = port::rounded(body, front, kRound);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  const double expected = port::volume_of(body) - 2.0 * kW * test::fillet_loss(kRound) + kept_within_fin();
  CHECK_NEAR_TOL(port::volume_of(result), expected, 1e-5);
  // C's crest grown up to the blend's contact on the top.
  CHECK(has_vertex_near(result, gp_Pnt(0, kRound, kH), 1e-3));
}

// The boundary: C's crest 0.01 below the top, the front's top edge one
// edge: not a mouth; where OCCT builds the rounding, it is valid.
void test_fin_below_the_top_is_not_a_mouth() {
  const TopoDS_Shape body = fin_below_top_body(0.01);
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> front = front_top_edges(body);
  CHECK(front.size() == 1);
  if (!port::valid(body) || front.size() != 1) {
    return;
  }
  const TopoDS_Shape result = port::rounded(body, front, kRound);
  if (result.IsNull()) {
    std::printf("p1210 boundary: the rounding over the fin below the top is not built\n");
    return;
  }
  CHECK(port::valid(result));
}

} // namespace

MITCAD_PORT_TEST("1210", test_rounding_over_a_slot_mouth_in_a_straight_wall);
MITCAD_PORT_TEST("1210", test_fin_below_the_top_is_not_a_mouth);
