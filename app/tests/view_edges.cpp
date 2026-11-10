// SPDX-License-Identifier: MIT
// The view's lines (mitcad#1633, view/ViewEdges.hpp): a body's edges are
// drawn from detached copies cut finely, whatever the body's size and its
// display meshes; the body's own edges get no 3D polygon (mitcad#1553);
// a highlight of an edge takes the body's copy, so that it lies on the
// polyline the body is drawn with; the same curve gives the same polyline.
#include "view/ViewEdges.hpp"
#include "view/ViewTessellation.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Polygon3D.hxx>
#include <Poly_PolygonOnTriangulation.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Trsf.hxx>

namespace {
int failures = 0;
void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "view_edges.cpp:%d: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(expr) check((expr), #expr, __LINE__)

constexpr double kPi = 3.14159265358979323846;

int polygons3d(const TopoDS_Shape& shape) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  int count = 0;
  for (int i = 1; i <= edges.Extent(); ++i) {
    TopLoc_Location location;
    if (!BRep_Tool::Polygon3D(TopoDS::Edge(edges(i)), location).IsNull()) {
      ++count;
    }
  }
  return count;
}

// The fewest segments a quarter turn and the largest sag of the copies'
// polylines on circles.
void circleCuts(const mitcad::ViewEdges& edges, int& circles, double& fewest, double& sag) {
  circles = 0;
  fewest = 1e9;
  sag = 0.0;
  for (const auto& [key, copy] : edges.copies()) {
    TopLoc_Location location;
    const occ::handle<Poly_Polygon3D>& polygon = BRep_Tool::Polygon3D(copy, location);
    CHECK(!polygon.IsNull());
    if (polygon.IsNull()) continue;
    const BRepAdaptor_Curve curve(copy);
    if (curve.GetType() != GeomAbs_Circle) continue;
    const double turn = curve.LastParameter() - curve.FirstParameter();
    ++circles;
    fewest = std::min(fewest, (polygon->NbNodes() - 1) / (turn / (0.5 * kPi)));
    const gp_Circ circle = curve.Circle();
    const NCollection_Array1<gp_Pnt>& nodes = polygon->Nodes();
    for (int i = nodes.Lower(); i < nodes.Upper(); ++i) {
      CHECK(std::abs(nodes(i).Distance(circle.Location()) - circle.Radius()) < 1e-6);
      const gp_Pnt middle((nodes(i).XYZ() + nodes(i + 1).XYZ()) * 0.5);
      sag = std::max(sag, circle.Radius() - middle.Distance(circle.Location()));
    }
  }
}

bool samePolyline(const TopoDS_Edge& a, const TopoDS_Edge& b) {
  TopLoc_Location la;
  TopLoc_Location lb;
  const occ::handle<Poly_Polygon3D>& pa = BRep_Tool::Polygon3D(a, la);
  const occ::handle<Poly_Polygon3D>& pb = BRep_Tool::Polygon3D(b, lb);
  if (pa.IsNull() || pb.IsNull() || pa->NbNodes() != pb->NbNodes()) return false;
  for (int i = 1; i <= pa->NbNodes(); ++i) {
    const gp_Pnt p = pa->Nodes()(i).Transformed(la.Transformation());
    const gp_Pnt q = pb->Nodes()(i).Transformed(lb.Transformation());
    if (p.Distance(q) > 1e-9) return false;
  }
  return true;
}

// A body of 400 mm with a hole edge of 3 mm: its display meshes cut the
// small circle in few pieces (small circles by size, mitcad#879, at the
// body's deviation of 0.5 mm, mitcad#1275); its drawn copies do not.
void fineLinesOfALargeBody() {
  const TopoDS_Shape body = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 200), gp::DZ()), 3.0, 400.0).Shape();
  const TopoDS_Shape large = BRepPrimAPI_MakeCylinder(200.0, 10.0).Shape();
  mitcad::io::MeshOptions options = mitcad::viewMeshOptions();
  options.small_circle_segments = mitcad::kViewSmallCircleSegments;
  CHECK(mitcad::io::mesh_in_place(body, options));
  CHECK(mitcad::io::mesh_in_place(large, options));
  // The meshes' polygons on the faces, not 3D polygons.
  CHECK(polygons3d(body) == 0);

  const mitcad::ViewEdges edges(body);
  CHECK(edges.size() == 3); // two circles and the seam
  int circles = 0;
  double fewest = 0.0;
  double sag = 0.0;
  circleCuts(edges, circles, fewest, sag);
  CHECK(circles == 2);
  // At least 90 pieces a turn: 22.5 a quarter.
  CHECK(fewest >= 22.0);
  CHECK(sag <= mitcad::kViewEdgeDeviation + 1e-9);
  // The body's own edges unwritten (mitcad#1553), their polygons on the
  // faces' meshes kept.
  CHECK(polygons3d(body) == 0);
  for (TopExp_Explorer it(body, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    occ::handle<Poly_PolygonOnTriangulation> polygon;
    occ::handle<Poly_Triangulation> triangulation;
    TopLoc_Location location;
    BRep_Tool::PolygonOnTriangulation(edge, polygon, triangulation, location);
    CHECK(!polygon.IsNull());
    // The copy is another edge with no polygon on a triangulation.
    const TopoDS_Edge copy = edges.copyOf(edge);
    CHECK(!copy.IsNull());
    CHECK(copy.TShape() != edge.TShape());
    occ::handle<Poly_PolygonOnTriangulation> copied;
    BRep_Tool::PolygonOnTriangulation(copy, copied, triangulation, location);
    CHECK(copied.IsNull());
  }
  // Drawn: every edge's copy where the body has it.
  int drawn = 0;
  for (TopExp_Explorer it(edges.drawn(), TopAbs_EDGE); it.More(); it.Next()) {
    ++drawn;
    CHECK(edges.copyOf(it.Current()).IsNull()); // copies, not the body's edges
  }
  CHECK(drawn == 3);

  // A large circle is cut by the deviation, finer than the angle.
  const mitcad::ViewEdges largeEdges(large);
  circleCuts(largeEdges, circles, fewest, sag);
  CHECK(circles == 2);
  CHECK(sag <= mitcad::kViewEdgeDeviation + 1e-9);
  CHECK(fewest > 90.0);
}

// A highlight of the body's edge, located as the view places it, takes
// the body's copy; one made without the body's copies cuts the same curve
// into the same polyline.
void highlightsOnTheBodysLines() {
  const TopoDS_Shape body = BRepPrimAPI_MakeCylinder(5.0, 20.0).Shape();
  const mitcad::ViewEdges edges(body);
  TopoDS_Edge circle;
  for (TopExp_Explorer it(body, TopAbs_EDGE); it.More() && circle.IsNull(); it.Next()) {
    if (BRepAdaptor_Curve(TopoDS::Edge(it.Current())).GetType() == GeomAbs_Circle) {
      circle = TopoDS::Edge(it.Current());
    }
  }
  CHECK(!circle.IsNull());
  if (circle.IsNull()) return;

  const mitcad::ViewEdges highlight(circle.Located(TopLoc_Location()), {&edges});
  CHECK(highlight.size() == 1);
  CHECK(highlight.copyOf(circle).TShape() == edges.copyOf(circle).TShape());
  const auto [both, shared] = highlight.sharedWith(edges);
  CHECK(both == 1);
  CHECK(shared == 1);

  const mitcad::ViewEdges alone(circle);
  CHECK(alone.copyOf(circle).TShape() != edges.copyOf(circle).TShape());
  CHECK(samePolyline(alone.copyOf(circle), edges.copyOf(circle)));
  const auto [aloneBoth, aloneShared] = alone.sharedWith(edges);
  CHECK(aloneBoth == 1);
  CHECK(aloneShared == 0);

  // A placed body: its drawn copies placed as its edges are.
  gp_Trsf move;
  move.SetTranslation(gp_Vec(0, 0, 200));
  const TopoDS_Shape placed = body.Moved(TopLoc_Location(move));
  const mitcad::ViewEdges placedEdges(placed, {&edges});
  CHECK(placedEdges.sharedWith(edges).second == edges.size());
  for (TopExp_Explorer it(placedEdges.drawn(), TopAbs_EDGE); it.More(); it.Next()) {
    TopLoc_Location location;
    const occ::handle<Poly_Polygon3D>& polygon = BRep_Tool::Polygon3D(TopoDS::Edge(it.Current()), location);
    CHECK(!polygon.IsNull());
    if (polygon.IsNull()) continue;
    const double z = polygon->Nodes()(1).Transformed(location.Transformation()).Z();
    CHECK(z > 199.0);
  }
}

// A sketch's own curves (mitcad#1586) get their polylines in place: an arc
// 200 mm from the origin cut as a body's edge on the same circle is.
void sketchCurvesCutAsBodyEdges() {
  const gp_Circ circle(gp_Ax2(gp_Pnt(56, 4, 200), gp::DZ()), 4.0);
  const TopoDS_Edge arc = BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(circle, 0.0, 0.5 * kPi, true).Value()).Edge();
  CHECK(polygons3d(arc) == 0);
  mitcad::giveViewEdgePolygons(arc);
  CHECK(polygons3d(arc) == 1);
  const mitcad::ViewEdges copy(arc);
  CHECK(samePolyline(arc, copy.copyOf(arc)));
  const TopoDS_Edge line = BRepBuilderAPI_MakeEdge(gp_Pnt(20, 24, 200), gp_Pnt(40, 16, 200)).Edge();
  mitcad::giveViewEdgePolygons(line);
  TopLoc_Location location;
  const occ::handle<Poly_Polygon3D>& straight = BRep_Tool::Polygon3D(line, location);
  CHECK(!straight.IsNull() && straight->NbNodes() == 2);
}
} // namespace

int main() {
  fineLinesOfALargeBody();
  highlightsOnTheBodysLines();
  sketchCurvesCutAsBodyEdges();
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("view edges: ok\n");
  return 0;
}
