// SPDX-License-Identifier: MIT
// Edges inside one smooth surface (mitcad#1773, smooth_seams.hpp): the
// views leave them out, every other edge stays.
//
// - A box: none of its 12 edges.
// - A cylinder: its seam, not its two circles. A sphere: its seam, not its
//   degenerated poles.
// - A box with a round hole through it: the hole's seam only.
// - A plate whose four upright edges are rounded: none of its 24 edges
//   (the roundings' boundaries meet the sides tangentially, on other
//   surfaces).
// - A teardrop (one closed B-spline with a sharp tip) extruded: none, its
//   seam is the crease at the tip. A closed smooth (periodic) spline and a
//   circle extruded: the seam.
// - Two faces on one plane sharing an edge: that edge; on two planes at
//   the same place: none.
// - Two boxes side by side fused (coplanar faces left on their own
//   planes): none without names; their top faces named as two pieces of
//   one face: the edge between them; named as two faces: none.
// - unsplit_face_name drops the trailing piece numbers only.

#include <cmath>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/smooth_seams.hpp"

namespace test {
namespace {

using mitcad::geometry::ShapeMap;
using mitcad::geometry::smooth_seams;

int edge_count(const TopoDS_Shape& shape) {
  ShapeMap edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  return edges.Extent();
}

// A face extruded 10 mm up from a closed wire in the XY plane.
TopoDS_Shape extruded(const TopoDS_Edge& closed) {
  const TopoDS_Wire wire = BRepBuilderAPI_MakeWire(closed).Wire();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire, true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0.0, 0.0, 10.0)).Shape();
}

void test_box() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(20.0, 20.0, 10.0).Shape();
  CHECK(edge_count(box) == 12);
  CHECK(smooth_seams(box).Extent() == 0);
}

void test_cylinder_and_sphere() {
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(5.0, 10.0).Shape();
  CHECK(edge_count(cylinder) == 3);
  const ShapeMap seams = smooth_seams(cylinder);
  CHECK(seams.Extent() == 1);
  if (seams.Extent() == 1) {
    // The seam runs up the side, not round it.
    const BRepAdaptor_Curve curve(TopoDS::Edge(seams(1)));
    CHECK(curve.GetType() == GeomAbs_Line);
  }
  const TopoDS_Shape sphere = BRepPrimAPI_MakeSphere(5.0).Shape();
  const ShapeMap meridian = smooth_seams(sphere);
  CHECK(meridian.Extent() == 1);
  if (meridian.Extent() == 1) {
    CHECK(!BRep_Tool::Degenerated(TopoDS::Edge(meridian(1))));
  }
}

void test_hole() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(20.0, 20.0, 10.0).Shape();
  const TopoDS_Shape tool =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10.0, 10.0, -1.0), gp::DZ()), 3.0, 12.0).Shape();
  const TopoDS_Shape holed = BRepAlgoAPI_Cut(box, tool).Shape();
  CHECK(edge_count(holed) == 15);
  CHECK(smooth_seams(holed).Extent() == 1);
}

void test_filleted_plate() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(60.0, 40.0, 5.0).Shape();
  BRepFilletAPI_MakeFillet fillet(plate);
  ShapeMap edges;
  TopExp::MapShapes(plate, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    if (curve.GetType() == GeomAbs_Line && curve.Line().Direction().IsParallel(gp::DZ(), 1e-9)) {
      fillet.Add(4.0, TopoDS::Edge(edges(i)));
    }
  }
  fillet.Build();
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape rounded = fillet.Shape();
  CHECK(edge_count(rounded) == 24);
  CHECK(smooth_seams(rounded).Extent() == 0);
}

void test_teardrop_and_smooth_profiles() {
  // A clamped cubic whose first and last poles are one point: closed, with
  // a sharp tip there.
  NCollection_Array1<gp_Pnt> poles(1, 5);
  poles(1) = gp_Pnt(0.0, 0.0, 0.0);
  poles(2) = gp_Pnt(20.0, 15.0, 0.0);
  poles(3) = gp_Pnt(40.0, 0.0, 0.0);
  poles(4) = gp_Pnt(20.0, -15.0, 0.0);
  poles(5) = gp_Pnt(0.0, 0.0, 0.0);
  NCollection_Array1<double> knots(1, 3);
  knots(1) = 0.0;
  knots(2) = 0.5;
  knots(3) = 1.0;
  NCollection_Array1<int> multiplicities(1, 3);
  multiplicities(1) = 4;
  multiplicities(2) = 1;
  multiplicities(3) = 4;
  const occ::handle<Geom_BSplineCurve> tip = new Geom_BSplineCurve(poles, knots, multiplicities, 3);
  const TopoDS_Shape teardrop = extruded(BRepBuilderAPI_MakeEdge(tip).Edge());
  CHECK(edge_count(teardrop) == 3);
  CHECK(smooth_seams(teardrop).Extent() == 0);

  // A periodic cubic through four points: smooth where it closes.
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, 4);
  points->SetValue(1, gp_Pnt(0.0, 0.0, 0.0));
  points->SetValue(2, gp_Pnt(20.0, 12.0, 0.0));
  points->SetValue(3, gp_Pnt(40.0, 0.0, 0.0));
  points->SetValue(4, gp_Pnt(20.0, -12.0, 0.0));
  GeomAPI_Interpolate interpolate(points, true, 1e-9);
  interpolate.Perform();
  CHECK(interpolate.IsDone());
  if (interpolate.IsDone()) {
    const TopoDS_Shape smooth = extruded(BRepBuilderAPI_MakeEdge(interpolate.Curve()).Edge());
    CHECK(edge_count(smooth) == 3);
    CHECK(smooth_seams(smooth).Extent() == 1);
  }

  const TopoDS_Shape round =
      extruded(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp::Origin(), gp::DZ()), 5.0)).Edge());
  CHECK(edge_count(round) == 3);
  CHECK(smooth_seams(round).Extent() == 1);
}

void test_one_plane() {
  // Vertices first, so that the edges share them and the wires take the
  // edges as they are (the shared edge one edge of both faces).
  const auto vertex = [](double x, double y) { return BRepBuilderAPI_MakeVertex(gp_Pnt(x, y, 0.0)).Vertex(); };
  const TopoDS_Vertex a = vertex(0.0, 0.0);
  const TopoDS_Vertex b = vertex(10.0, 0.0);
  const TopoDS_Vertex c = vertex(10.0, 10.0);
  const TopoDS_Vertex d = vertex(0.0, 10.0);
  const TopoDS_Vertex e = vertex(20.0, 0.0);
  const TopoDS_Vertex f = vertex(20.0, 10.0);
  const auto edge = [](const TopoDS_Vertex& from, const TopoDS_Vertex& to) {
    return BRepBuilderAPI_MakeEdge(from, to).Edge();
  };
  const TopoDS_Edge shared = edge(b, c);
  const TopoDS_Wire left = BRepBuilderAPI_MakeWire(edge(a, b), shared, edge(c, d), edge(d, a)).Wire();
  const TopoDS_Wire right =
      BRepBuilderAPI_MakeWire(edge(b, e), edge(e, f), edge(f, c), TopoDS::Edge(shared.Reversed())).Wire();
  const occ::handle<Geom_Plane> plane = new Geom_Plane(gp_Pln(gp::XOY()));
  const auto pair = [&](const occ::handle<Geom_Plane>& second) {
    TopoDS_Compound both;
    BRep_Builder builder;
    builder.MakeCompound(both);
    builder.Add(both, BRepBuilderAPI_MakeFace(plane, left, true).Face());
    builder.Add(both, BRepBuilderAPI_MakeFace(second, right, true).Face());
    return both;
  };
  const ShapeMap one = smooth_seams(pair(plane));
  CHECK(one.Extent() == 1);
  if (one.Extent() == 1) {
    CHECK(one(1).IsSame(shared));
  }
  const occ::handle<Geom_Plane> other = new Geom_Plane(gp_Pln(gp::XOY()));
  CHECK(smooth_seams(pair(other)).Extent() == 0);
}

void test_named_pieces() {
  const TopoDS_Shape first = BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), 10.0, 10.0, 10.0).Shape();
  const TopoDS_Shape second = BRepPrimAPI_MakeBox(gp_Pnt(10.0, 0.0, 0.0), 10.0, 10.0, 10.0).Shape();
  const TopoDS_Shape fused = BRepAlgoAPI_Fuse(first, second).Shape();
  CHECK(smooth_seams(fused).Extent() == 0);
  // The two top faces (at z 10).
  std::vector<TopoDS_Shape> tops;
  for (TopExp_Explorer it(fused, TopAbs_FACE); it.More(); it.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()));
    const gp_Pnt middle = surface.Value(0.5 * (surface.FirstUParameter() + surface.LastUParameter()),
                                        0.5 * (surface.FirstVParameter() + surface.LastVParameter()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(middle.Z() - 10.0) < 1e-9) {
      tops.push_back(it.Current());
    }
  }
  CHECK(tops.size() == 2);
  if (tops.size() != 2) {
    return;
  }
  const mitcad::geometry::Shape pieces(fused, {{tops[0], {"F1:top#0"}}, {tops[1], {"F1:top#1"}}});
  CHECK(smooth_seams(pieces.occt(), &pieces).Extent() == 1);
  const mitcad::geometry::Shape apart(fused, {{tops[0], {"F1:top#0"}}, {tops[1], {"F2:top#0"}}});
  CHECK(smooth_seams(apart.occt(), &apart).Extent() == 0);
}

void test_unsplit_names() {
  using mitcad::geometry::unsplit_face_name;
  CHECK(unsplit_face_name("F5:fillet(E{F2:a|F2:b})#0") == "F5:fillet(E{F2:a|F2:b})");
  CHECK(unsplit_face_name("F3:side0#0#1") == "F3:side0");
  CHECK(unsplit_face_name("F3:side0") == "F3:side0");
  CHECK(unsplit_face_name("F3:side(c1[c4,c2])") == "F3:side(c1[c4,c2])");
  CHECK(unsplit_face_name("F4:inst1(F2:side(c6[c17,c18])#1)") == "F4:inst1(F2:side(c6[c17,c18])#1)");
}

} // namespace
} // namespace test

int main() {
  test::guarded("box", test::test_box);
  test::guarded("cylinder and sphere", test::test_cylinder_and_sphere);
  test::guarded("hole", test::test_hole);
  test::guarded("filleted plate", test::test_filleted_plate);
  test::guarded("teardrop and smooth profiles", test::test_teardrop_and_smooth_profiles);
  test::guarded("one plane", test::test_one_plane);
  test::guarded("named pieces", test::test_named_pieces);
  test::guarded("unsplit names", test::test_unsplit_names);
  if (test::failures == 0) {
    std::puts("test_smooth_seams: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
