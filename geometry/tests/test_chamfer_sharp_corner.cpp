// SPDX-License-Identifier: MIT
// The corner of a convex and a concave chamfer at a vertex where a third,
// sharp edge goes on, built with OCCT's chamfer alone. An L bracket: a
// 60 x 40 x 5 floor and a 60 x 5 x 40 wall at its back, fused, its
// coplanar faces merged. The floor's top edge along one end (convex) and
// the floor's edge along the wall (concave) are chamfered 1; at their common
// vertex the wall's sharp vertical edge goes on up. OCCT closes such a
// corner with a plate between the two bevels' ends and the sharp edge,
// limited on that edge by the bevels' common points (PerformMoreThreeCorner).
// The convex bevel's end is OnSame: its common point there was moved onto
// the sharp edge's prolongation below the vertex, but kept the parameter it
// had on the edge its contact ran to (the floor's far vertical edge). At one
// end of the bracket that common point won the tie of the two points' equal
// distances to the vertex, by rounding, and its parameter was read on the
// sharp edge's curve: the plate was bounded 31 above the floor on the wall,
// a 122 mm2 B-spline of degree 8 waving over the bracket's end that took
// seconds to mesh, and the body lost 16.6 mm3 there. The two ends of the
// bracket now get the same corner, the size of the bevels. The case behind
// the patch 1090 of the OCCT port (third_party/vcpkg-ports, mitcad#1005).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepGProp.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Poly_Triangulation.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kLength = 60.0; // along X
constexpr double kWidth  = 40.0; // along Y
constexpr double kFloor  = 5.0;  // the floor's height
constexpr double kWall   = 5.0;  // the wall's thickness, at Y 35 to 40
constexpr double kHeight = 40.0; // the wall's height
constexpr double kD      = 1.0;  // the chamfer

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

TopoDS_Shape bracket() {
  const TopoDS_Shape floor = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), kLength, kWidth, kFloor).Shape();
  const TopoDS_Shape wall =
    BRepPrimAPI_MakeBox(gp_Pnt(0, kWidth - kWall, 0), kLength, kWall, kHeight).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(floor, wall).Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

// The straight edge of the body from a to b (either way round).
TopoDS_Edge edge_between(const TopoDS_Shape& body, const gp_Pnt& a, const gp_Pnt& b) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    double f = 0., l = 0.;
    const auto c = BRep_Tool::Curve(e, f, l);
    if (c.IsNull()) {
      continue;
    }
    const gp_Pnt p = c->Value(f), q = c->Value(l);
    if ((p.Distance(a) < 1e-7 && q.Distance(b) < 1e-7) || (p.Distance(b) < 1e-7 && q.Distance(a) < 1e-7)) {
      return e;
    }
  }
  return TopoDS_Edge();
}

// The floor's top edge along the end at x, and the concave edge along the
// wall.
TopoDS_Edge end_edge(const TopoDS_Shape& body, const double x) {
  return edge_between(body, gp_Pnt(x, 0, kFloor), gp_Pnt(x, kWidth - kWall, kFloor));
}

TopoDS_Edge concave_edge(const TopoDS_Shape& body) {
  return edge_between(body, gp_Pnt(0, kWidth - kWall, kFloor), gp_Pnt(kLength, kWidth - kWall, kFloor));
}

struct Result {
  double volume = 0.;
  double largest_curved = 0.; // the largest extent of a face not a plane
  int    curved_triangles = 0; // the triangles of those faces, meshed at 0.01
};

Result chamfer_case(const char* name, const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges) {
  Result                    out;
  BRepFilletAPI_MakeChamfer chamfer(body);
  for (const TopoDS_Edge& e : edges) {
    CHECK(!e.IsNull());
    if (e.IsNull()) {
      return out;
    }
    chamfer.Add(kD, e);
  }
  try {
    OCC_CATCH_SIGNALS
    chamfer.Build();
  } catch (const Standard_Failure& error) {
    std::printf("%s: %s\n", name, error.what());
  }
  CHECK(chamfer.IsDone());
  if (!chamfer.IsDone()) {
    std::printf("%s: not done\n", name);
    return out;
  }
  const TopoDS_Shape result = chamfer.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  out.volume = volume_of(result);
  // (Meshed as a copy: the result shares its untouched faces with the body.)
  const TopoDS_Shape       meshed = BRepBuilderAPI_Copy(result).Shape();
  BRepMesh_IncrementalMesh mesh(meshed, 0.01, false, 5.0 * kPi / 180.0);
  for (TopExp_Explorer ex(meshed, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    if (BRepAdaptor_Surface(face).GetType() == GeomAbs_Plane) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::AddOptimal(face, box, false);
    double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
    box.Get(x0, y0, z0, x1, y1, z1);
    out.largest_curved = std::max({out.largest_curved, x1 - x0, y1 - y0, z1 - z0});
    TopLoc_Location loc;
    const auto      tri = BRep_Tool::Triangulation(face, loc);
    out.curved_triangles += tri.IsNull() ? 0 : tri->NbTriangles();
  }
  std::printf("%s: volume %.9f, largest corner %.6f, its triangles %d\n", name, out.volume, out.largest_curved,
              out.curved_triangles);
  // A corner between bevels of 1: within the bevels' reach of the vertex,
  // from the convex bevel's foot 1 below the floor's top to the concave
  // one's 1 up the wall (it was 32 high), and about a thousand triangles (it
  // was 86 000).
  CHECK(out.largest_curved < 2.0 * kD + 1e-6);
  CHECK(out.curved_triangles < 5000);
  return out;
}

void test_convex_and_concave_bevel_at_a_sharp_edge() {
  const TopoDS_Shape body = bracket();
  const TopoDS_Edge  left = end_edge(body, 0.0), right = end_edge(body, kLength), back = concave_edge(body);
  const Result l1 = chamfer_case("the left end, its edge first", body, {left, back});
  const Result l2 = chamfer_case("the left end, the concave edge first", body, {back, left});
  const Result r1 = chamfer_case("the right end, its edge first", body, {right, back});
  const Result r2 = chamfer_case("the right end, the concave edge first", body, {back, right});
  // Both ends the same corner, mirrored.
  CHECK_NEAR_TOL(l1.volume, r1.volume, 1e-9);
  CHECK_NEAR_TOL(l2.volume, r2.volume, 1e-9);
  CHECK_NEAR_TOL(l1.volume, l2.volume, 1e-9);
  // The bevels' prisms: the end's taken away, the wall's added; the corner
  // plate within 0.1 mm3 of where they meet.
  const double prisms = volume_of(body) - 0.5 * kD * kD * (kWidth - kWall) + 0.5 * kD * kD * kLength;
  CHECK(std::abs(l1.volume - prisms) < 0.1);
  // The floor's top face: all four edges, both ends' corners. At each front
  // corner the two bevels' prisms overlap in a pyramid of a third of kD^3,
  // taken away once.
  const TopoDS_Edge front = edge_between(body, gp_Pnt(0, 0, kFloor), gp_Pnt(kLength, 0, kFloor));
  const Result all = chamfer_case("the floor's top face", body, {front, left, right, back});
  const double allPrisms = volume_of(body) - 0.5 * kD * kD * (kLength + 2.0 * (kWidth - kWall)) +
                           0.5 * kD * kD * kLength + 2.0 * kD * kD * kD / 3.0;
  CHECK(std::abs(all.volume - allPrisms) < 0.1);
  // Its back corners those of the single ends (their plates within a
  // thousandth of a mm3).
  CHECK_NEAR_TOL(all.volume - allPrisms, 2.0 * (l1.volume - prisms), 1e-3);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_convex_and_concave_bevel_at_a_sharp_edge", test::test_convex_and_concave_bevel_at_a_sharp_edge);
  if (test::failures == 0) {
    std::puts("test_chamfer_sharp_corner: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
