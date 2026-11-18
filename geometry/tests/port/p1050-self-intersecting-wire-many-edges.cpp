// SPDX-License-Identifier: MIT
// Port patch 1050: ShapeFix_IntersectionTool::FixSelfIntersectWire reads the
// wire's edges into an array once (again after each pair of edges that
// intersect) instead of by number from the wire's sequence for every pair of
// edges, whose access walks node by node from the place read last: the
// search grew with the cube of the edges. The fixes are the ones of stock
// OCCT: a closed polygon whose crossing chords are split at the crossing,
// two crossings far apart both split (the array read again after the first),
// a polygon without crossings left as it is, and a polygon of 6 000 edges
// fixed in time.

#include <chrono>
#include <cmath>
#include <cstddef>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeExtend_WireData.hxx>
#include <ShapeFix_IntersectionTool.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 100.0;

// The corners of a polygon of n corners on a circle of radius 100 round the
// origin in the plane z = 0, counter-clockwise, with corners k and k + 1
// swapped for each k of `crossings`: the chords from corner k - 1 to k + 1
// and from k to k + 2 then cross in their middles.
std::vector<gp_Pnt> corners(int n, const std::vector<int>& crossings) {
  std::vector<gp_Pnt> points;
  for (int i = 0; i < n; ++i) {
    const double angle = 2.0 * test::kPi * i / n;
    points.emplace_back(kRadius * std::cos(angle), kRadius * std::sin(angle), 0.0);
  }
  for (const int k : crossings) {
    std::swap(points[static_cast<std::size_t>(k)], points[static_cast<std::size_t>(k) + 1]);
  }
  return points;
}

TopoDS_Wire polygon(const std::vector<gp_Pnt>& points) {
  BRepBuilderAPI_MakePolygon maker;
  for (const gp_Pnt& point : points) {
    maker.Add(point);
  }
  maker.Close();
  return maker.Wire();
}

// The point where the chords p1-p2 and q1-q2 of the plane z = 0 cross.
gp_Pnt crossing_of(const gp_Pnt& p1, const gp_Pnt& p2, const gp_Pnt& q1, const gp_Pnt& q2) {
  const double dx1 = p2.X() - p1.X(), dy1 = p2.Y() - p1.Y();
  const double dx2 = q2.X() - q1.X(), dy2 = q2.Y() - q1.Y();
  const double t = ((q1.X() - p1.X()) * dy2 - (q1.Y() - p1.Y()) * dx2) / (dx1 * dy2 - dy1 * dx2);
  return gp_Pnt(p1.X() + t * dx1, p1.Y() + t * dy1, 0.0);
}

struct Fixed {
  bool                                                          done    = false;
  int                                                           split   = 0;
  int                                                           cut     = 0;
  int                                                           removed = 0;
  occ::handle<ShapeExtend_WireData>                             wire;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> original_edges;
  double                                                        seconds = 0.0;
};

// The wire's self-intersections fixed on a face of the plane z = 0 bounded
// by it, as ShapeFix_Wire fixes them.
Fixed fixed(const std::vector<gp_Pnt>& points) {
  Fixed             result;
  const TopoDS_Wire wire = polygon(points);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(gp_Pln(), wire).Face();
  TopExp::MapShapes(wire, TopAbs_EDGE, result.original_edges);
  result.wire = new ShapeExtend_WireData(wire);
  const occ::handle<ShapeBuild_ReShape> context(new ShapeBuild_ReShape());
  const ShapeFix_IntersectionTool       tool(context, 1e-7, 1.0);
  const auto                            start = std::chrono::steady_clock::now();
  result.done    = tool.FixSelfIntersectWire(result.wire, face, result.split, result.cut, result.removed);
  result.seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  return result;
}

// The edges of the fixed wire that are not edges of the original one.
std::vector<TopoDS_Edge> new_edges(const Fixed& result) {
  std::vector<TopoDS_Edge> edges;
  for (int i = 1; i <= result.wire->NbEdges(); ++i) {
    const TopoDS_Edge edge = result.wire->Edge(i);
    if (!result.original_edges.Contains(edge)) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// The new edges that end at one vertex at the point.
int new_edges_at(const std::vector<TopoDS_Edge>& edges, const gp_Pnt& point) {
  TopoDS_Vertex common;
  int           count = 0;
  for (const TopoDS_Edge& edge : edges) {
    for (TopExp_Explorer vertices(edge, TopAbs_VERTEX); vertices.More(); vertices.Next()) {
      const TopoDS_Vertex& vertex = TopoDS::Vertex(vertices.Current());
      if (BRep_Tool::Pnt(vertex).Distance(point) < 1e-6) {
        if (common.IsNull()) {
          common = vertex;
        }
        if (vertex.IsSame(common)) {
          ++count;
        }
        break;
      }
    }
  }
  return count;
}

// One crossing: both chords split at the crossing point (four new edges at
// one new vertex), the other edges as they were.
void test_crossing_chords_are_split() {
  const int                 n      = 200;
  const std::vector<gp_Pnt> points = corners(n, {100});
  const Fixed               result = fixed(points);
  CHECK(result.done);
  CHECK(result.split == 2);
  CHECK(result.cut == 0);
  CHECK(result.removed == 0);
  CHECK(result.wire->NbEdges() == n + 2);
  const std::vector<TopoDS_Edge> added = new_edges(result);
  CHECK(added.size() == 4);
  CHECK(new_edges_at(added, crossing_of(points[99], points[100], points[101], points[102])) == 4);
}

// Two crossings far apart: the second pair is found after the first fix
// changed the wire (the edges read again), and split alike.
void test_two_crossings_far_apart_are_both_split() {
  const int                 n      = 300;
  const std::vector<gp_Pnt> points = corners(n, {20, 240});
  const Fixed               result = fixed(points);
  CHECK(result.done);
  CHECK(result.split == 4);
  CHECK(result.cut == 0);
  CHECK(result.removed == 0);
  CHECK(result.wire->NbEdges() == n + 4);
  const std::vector<TopoDS_Edge> added = new_edges(result);
  CHECK(added.size() == 8);
  CHECK(new_edges_at(added, crossing_of(points[19], points[20], points[21], points[22])) == 4);
  CHECK(new_edges_at(added, crossing_of(points[239], points[240], points[241], points[242])) == 4);
}

// A polygon without crossings: nothing to fix, the wire as it was.
void test_polygon_without_crossings_is_left_as_it_is() {
  const int   n      = 300;
  const Fixed result = fixed(corners(n, {}));
  CHECK(!result.done);
  CHECK(result.split == 0);
  CHECK(result.wire->NbEdges() == n);
  CHECK(new_edges(result).empty());
}

// The bound: 6 000 edges with one crossing in the middle. Reading each
// pair's two edges by number from the sequence walked about n^3 / 6 nodes
// (3.6e10 here, tens of seconds); the 1.8e7 pairs' box tests take one to
// two seconds, so 6 s leaves room for a loaded machine.
void test_six_thousand_edges_fixed_within_six_seconds() {
  const int                 n      = 6000;
  const std::vector<gp_Pnt> points = corners(n, {3000});
  const Fixed               result = fixed(points);
  std::printf("self-intersections of a wire of %d edges: %.2f s\n", n, result.seconds);
  CHECK(result.done);
  CHECK(result.split == 2);
  CHECK(result.wire->NbEdges() == n + 2);
  CHECK(new_edges_at(new_edges(result), crossing_of(points[2999], points[3000], points[3001], points[3002])) == 4);
  CHECK(result.seconds < 6.0);
}

} // namespace

MITCAD_PORT_TEST("1050", test_crossing_chords_are_split);
MITCAD_PORT_TEST("1050", test_two_crossings_far_apart_are_both_split);
MITCAD_PORT_TEST("1050", test_polygon_without_crossings_is_left_as_it_is);
MITCAD_PORT_TEST("1050", test_six_thousand_edges_fixed_within_six_seconds);
