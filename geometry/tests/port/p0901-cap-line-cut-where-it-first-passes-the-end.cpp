// SPDX-License-Identifier: MIT
// Port patch 0901: the walked intersection of a blend with the face at its
// end (the cap) ends where the walk first passes the end point, not where
// it last comes back near it.
//
// The roof slab of test_fillet_eave.cpp (a section of seven points
// extruded along x): its eave is a level face 1.506 wide between the tip's
// edge (60 degrees) and an obtuse edge. On the end face x = 0 the tip's
// edge, the eave's end edge and the slope's top edge are rounded with 0.9
// or 1.0: at the tip the three blends cut each other, and the caps are
// walked where the blends touch the faces at their ends. Where the walk
// passes through such an end onto the intersection's other branch and
// comes back near the end, the cut kept the wandering stretch and the
// result came out invalid: before the patch, 9 of the 101 slabs 20 to 30
// long (every 0.1) rounded with 0.9 and 5 rounded with 1.0, which of them
// hanging on rounding noise of the walk (with 0.9 the lengths 20.1, 20.5,
// 21.5, 23.9, 24.4, 24.6, 26.3, 29.5 and 30 on one machine). Near such an
// end the walk may also step back and forth across it (the slab 28.0 long
// rounded with 1.0 on Windows, mitcad#2151): the line passes the end inside
// that step, not at the nearer point it steps back to. With the patch every
// one is a valid closed solid.

#include <cmath>
#include <utility>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <gp_Pnt2d.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRise = 2.6 / 1.506; // the slopes' dz / dy
constexpr double kEave = 17.8;        // the eave's height
constexpr double kTip   = -14.906;    // the eave's tip (y)
constexpr double kInner = -13.4;      // the eave's obtuse edge (y)

// The slab's section (y, z).
std::vector<gp_Pnt2d> slab_section() {
  const double ridge        = kEave + (10.9 - kTip) * kRise;
  const double ceiling      = ridge - 2.6 - 3.0;
  const double ceiling_left = kInner + (ceiling - kEave) / kRise;
  return {gp_Pnt2d(kTip, kEave),
          gp_Pnt2d(kInner, kEave),
          gp_Pnt2d(ceiling_left, ceiling),
          gp_Pnt2d(21.8 - ceiling_left, ceiling),
          gp_Pnt2d(35.2, kEave),
          gp_Pnt2d(36.706, kEave),
          gp_Pnt2d(10.9, ridge)};
}

TopoDS_Shape slab(double length) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt2d& p : slab_section()) {
    polygon.Add(gp_Pnt(0.0, p.X(), p.Y()));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(length, 0.0, 0.0)).Shape();
}

// The edge from a to b (either way), or none.
std::vector<TopoDS_Edge> edge_between(const TopoDS_Shape& body, const gp_Pnt& a, const gp_Pnt& b) {
  std::vector<TopoDS_Edge> found;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    TopoDS_Vertex v1, v2;
    TopExp::Vertices(TopoDS::Edge(edges(i)), v1, v2);
    const gp_Pnt p1 = BRep_Tool::Pnt(v1), p2 = BRep_Tool::Pnt(v2);
    if ((p1.Distance(a) < 1e-6 && p2.Distance(b) < 1e-6) || (p1.Distance(b) < 1e-6 && p2.Distance(a) < 1e-6)) {
      found.push_back(TopoDS::Edge(edges(i)));
    }
  }
  return found;
}

// The tip's edge along x, the eave's end edge and the slope's top edge on
// the end face x = 0.
std::vector<TopoDS_Edge> tip_corner_edges(const TopoDS_Shape& body, double length) {
  const std::vector<gp_Pnt2d> s = slab_section();
  const auto at = [&s](int i, double x) { return gp_Pnt(x, s[i].X(), s[i].Y()); };
  std::vector<TopoDS_Edge> edges;
  for (const auto& [a, b] : {std::pair{at(0, 0), at(0, length)}, {at(0, 0), at(1, 0)}, {at(6, 0), at(0, 0)}}) {
    for (const TopoDS_Edge& e : edge_between(body, a, b)) {
      edges.push_back(e);
    }
  }
  return edges;
}

// The tip's corner of slabs 20 to 30 long, every 0.1, rounded with r: the
// number of results that are not one valid solid.
int bad_corners(double r) {
  int bad = 0;
  for (int i = 0; i <= 100; ++i) {
    const double                   length = 20.0 + 0.1 * i;
    const TopoDS_Shape             body   = slab(length);
    const std::vector<TopoDS_Edge> edges  = tip_corner_edges(body, length);
    CHECK(edges.size() == 3);
    const TopoDS_Shape rounded = port::rounded(body, edges, r);
    if (!port::valid(rounded) || port::count_of(rounded, TopAbs_SOLID) != 1) {
      std::printf("  length %.1f: not one valid solid\n", length);
      ++bad;
    }
  }
  std::printf("rounded with %.1f: %d of 101 slabs not one valid solid\n", r, bad);
  return bad;
}

void test_tip_corners_rounded_with_0_9_are_valid() {
  CHECK(bad_corners(0.9) == 0);
}

// The tip's ball of 1.0 rolls on the obtuse edge further from the tip.
void test_tip_corners_rounded_with_1_0_are_valid() {
  CHECK(bad_corners(1.0) == 0);
}

} // namespace

MITCAD_PORT_TEST("0901", test_tip_corners_rounded_with_0_9_are_valid);
MITCAD_PORT_TEST("0901", test_tip_corners_rounded_with_1_0_are_valid);
