// SPDX-License-Identifier: MIT
// Exercise the viewport's OCCT meshing path on synthetic STEP round trips.
#include "view/ViewTessellation.hpp"
#include "mitcad/io/step.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <filesystem>
#include <limits>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <gp_Pln.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Tool.hxx>
#include <Geom_BSplineSurface.hxx>
#include <IMeshTools_Parameters.hxx>
#include <NCollection_Sequence.hxx>
#include <Precision.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Poly_PolygonOnTriangulation.hxx>
#include <Poly_Triangulation.hxx>
#include <StdPrs_DeflectionCurve.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <StdPrs_ToolTriangulatedShape.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>

namespace {
int failures = 0;
void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "view_tessellation.cpp:%d: %s\n", line, expression);
    ++failures;
  }
}
#define CHECK(expr) check((expr), #expr, __LINE__)

double sag(const gp_Pnt& a, const gp_Pnt& b, const gp_Circ& circle) {
  const gp_Pnt middle((a.XYZ() + b.XYZ()) * 0.5);
  return circle.Radius() - middle.Distance(circle.Location());
}

void checkCylinder(const std::string& path, double radius, double height) {
  mitcad::io::Body body;
  body.name = "Cylinder";
  body.shape = BRepPrimAPI_MakeCylinder(radius, height).Shape();
  mitcad::io::write_step(path, {body});
  const auto imported = mitcad::io::read_step(path);
  CHECK(imported.size() == 1);
  if (imported.size() != 1) return;
  const TopoDS_Shape& shape = imported.front().shape;
  const occ::handle<Prs3d_Drawer> defaults = new Prs3d_Drawer();
  defaults->SetupOwnDefaults();
  mitcad::setViewTessellation(defaults);
  // AIS objects inherit the context's drawer instead of owning tolerances.
  const occ::handle<Prs3d_Drawer> drawer = new Prs3d_Drawer();
  drawer->SetLink(defaults);
  CHECK(StdPrs_ToolTriangulatedShape::Tessellate(shape, drawer));
  int circles = 0;
  double boundarySag = 0.0;
  double wireSag = 0.0;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    TopLoc_Location location;
    const auto mesh = BRep_Tool::Triangulation(TopoDS::Face(face.Current()), location);
    CHECK(!mesh.IsNull());
    if (mesh.IsNull()) continue;
    for (TopExp_Explorer edge(face.Current(), TopAbs_EDGE); edge.More(); edge.Next()) {
      const TopoDS_Edge& boundary = TopoDS::Edge(edge.Current());
      BRepAdaptor_Curve curve(boundary);
      if (curve.GetType() != GeomAbs_Circle) continue;
      ++circles;
      CHECK(std::abs(curve.Circle().Radius() - radius) < radius * 1e-6);
      const auto polygon = BRep_Tool::PolygonOnTriangulation(boundary, mesh, location);
      CHECK(!polygon.IsNull());
      if (polygon.IsNull()) continue;
      for (int n = 2; n <= polygon->NbNodes(); ++n) {
        const gp_Pnt a = mesh->Node(polygon->Node(n - 1)).Transformed(location.Transformation());
        const gp_Pnt b = mesh->Node(polygon->Node(n)).Transformed(location.Transformation());
        // At a 500-pixel radius the boundary deviates by less than a
        // pixel, including a small circle on a very long imported body
        // (half a pixel at 1 000 pixels before mitcad#1275: the view's
        // edges are cut in steps of 6 degrees now, 2.5 before).
        boundarySag = std::max(boundarySag, sag(a, b, curve.Circle()));
      }
      NCollection_Sequence<gp_Pnt> wire;
      StdPrs_DeflectionCurve::Add({}, curve,
          StdPrs_ToolTriangulatedShape::GetDeflection(shape, drawer), drawer, wire, false);
      CHECK(wire.Length() > 2);
      for (int n = 2; n <= wire.Length(); ++n) {
        wireSag = std::max(wireSag, sag(wire.Value(n - 1), wire.Value(n), curve.Circle()));
      }
    }
  }
  CHECK(circles >= 2);
  std::printf("Cylinder radius %g, height %g: boundary deviation %g, wire deviation %g\n",
              radius, height, boundarySag, wireSag);
  CHECK(boundarySag <= radius * 0.0015);
  CHECK(wireSag <= radius * 0.001);
}

// A hemispherical dome shell (mitcad#398): sphere faces of radii 4450 and
// 4550 mm times `scale`, and a flat ring.
TopoDS_Shape dome(double scale) {
  const double quarter = 2.0 * std::atan(1.0);
  const TopoDS_Shape outer = BRepPrimAPI_MakeSphere(gp_Ax2(), 4550.0 * scale, 0.0, quarter).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeSphere(gp_Ax2(), 4450.0 * scale, 0.0, quarter).Shape();
  return BRepAlgoAPI_Cut(outer, inner).Shape();
}

std::vector<occ::handle<Poly_Triangulation>> triangulations(const TopoDS_Shape& shape) {
  std::vector<occ::handle<Poly_Triangulation>> meshes;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    TopLoc_Location location;
    meshes.push_back(BRep_Tool::Triangulation(TopoDS::Face(face.Current()), location));
  }
  return meshes;
}

// Each face's triangle count (-1 without a triangulation).
std::vector<int> triangleCounts(const TopoDS_Shape& shape) {
  std::vector<int> counts;
  for (const auto& mesh : triangulations(shape)) {
    counts.push_back(mesh.IsNull() ? -1 : mesh->NbTriangles());
  }
  return counts;
}

int sphereTriangles(const TopoDS_Shape& shape, int& spheres) {
  int largest = 0;
  spheres = 0;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    TopLoc_Location location;
    const TopoDS_Face& f = TopoDS::Face(face.Current());
    const auto mesh = BRep_Tool::Triangulation(f, location);
    if (BRepAdaptor_Surface(f).GetType() == GeomAbs_Sphere && !mesh.IsNull()) {
      ++spheres;
      largest = std::max(largest, mesh->NbTriangles());
    }
  }
  return largest;
}

// The view's meshes of a building-scale dome (mitcad#398): at an absolute
// 0.01 mm its sphere faces took millions of triangles on the UI thread;
// the view's drawer now gives each kViewFaceTriangles at most (held within
// it since mitcad#1275; about 1.5 times as many before), keeps the meshes
// made on the model's thread (mesh_in_place), and meshes a dome of 3.6 cm
// exactly as plain BRepMesh does at the dome's deviation by its size
// (0.04 mm for its 55 mm diagonal, mitcad#1275; an absolute 0.01 mm
// before). Counts, not times.
void checkDome(const occ::handle<Prs3d_Drawer>& drawer) {
  const TopoDS_Shape big = dome(1.0);
  CHECK(StdPrs_ToolTriangulatedShape::Tessellate(big, drawer));
  int spheres = 0;
  const int drawn = sphereTriangles(big, spheres);
  std::printf("Dome of 9 m in the view: %d triangles on the larger sphere face\n", drawn);
  CHECK(spheres == 2);
  CHECK(drawn > 5000 && drawn <= static_cast<int>(mitcad::kViewFaceTriangles));

  // Meshed on the model's thread first: the view keeps those meshes.
  const TopoDS_Shape shown = dome(1.0);
  CHECK(mitcad::io::mesh_in_place(shown, mitcad::viewMeshOptions()));
  const auto before = triangulations(shown);
  StdPrs_ToolTriangulatedShape::Tessellate(shown, drawer);
  CHECK(triangulations(shown) == before);

  // A small dome (radii 17.8 and 18.2 mm): as plain BRepMesh with the
  // view's deviation for its size and the view's angles.
  const TopoDS_Shape small = dome(0.004);
  const TopoDS_Shape plain = BRepBuilderAPI_Copy(small, false, false).Shape();
  const double smallDeviation = mitcad::viewShapeDeviation(small);
  std::printf("Dome of 3.6 cm: deviation %g mm\n", smallDeviation);
  CHECK(std::abs(smallDeviation - 0.04) < 1e-9);
  CHECK(StdPrs_ToolTriangulatedShape::Tessellate(small, drawer));
  IMeshTools_Parameters plainParameters;
  plainParameters.Deflection = smallDeviation;
  plainParameters.Angle = mitcad::kViewAngle;
  plainParameters.AngleInterior = mitcad::kViewInteriorAngle;
  plainParameters.InParallel = true;
  BRepMesh_IncrementalMesh(plain, plainParameters);
  std::vector<int> counts;
  std::vector<int> plainCounts;
  for (const auto& mesh : triangulations(small)) {
    counts.push_back(mesh.IsNull() ? -1 : mesh->NbTriangles());
  }
  for (const auto& mesh : triangulations(plain)) {
    plainCounts.push_back(mesh.IsNull() ? -1 : mesh->NbTriangles());
  }
  CHECK(counts == plainCounts);

  // Both domes meshed at once on the model's thread (mitcad#438, all the
  // bodies of a job together): the meshes each gets alone, and the view
  // keeps them.
  const std::vector<TopoDS_Shape> together{dome(1.0), dome(0.004)};
  CHECK(mitcad::io::mesh_all_in_place(together, mitcad::viewMeshOptions()) == together.size());
  CHECK(triangleCounts(together[0]) == triangleCounts(shown));
  CHECK(triangleCounts(together[1]) == counts);
  for (const TopoDS_Shape& body : together) {
    const auto made = triangulations(body);
    StdPrs_ToolTriangulatedShape::Tessellate(body, drawer);
    CHECK(triangulations(body) == made);
  }
}

// The most points a circle edge has in the triangulations of its faces.
int circlePoints(const TopoDS_Shape& shape) {
  int most = 0;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    TopLoc_Location location;
    const auto mesh = BRep_Tool::Triangulation(TopoDS::Face(face.Current()), location);
    if (mesh.IsNull()) continue;
    for (TopExp_Explorer edge(face.Current(), TopAbs_EDGE); edge.More(); edge.Next()) {
      const TopoDS_Edge& boundary = TopoDS::Edge(edge.Current());
      if (BRepAdaptor_Curve(boundary).GetType() != GeomAbs_Circle) continue;
      const auto polygon = BRep_Tool::PolygonOnTriangulation(boundary, mesh, location);
      if (!polygon.IsNull()) most = std::max(most, polygon->NbNodes());
    }
  }
  return most;
}

// A tank of 5 m radius in the view (mitcad#438): its circles at most as
// fine as a turn in 500 pieces (2e-5 of the radius; about 700 points, as
// BRepMesh cuts edges at half their deviation), not the 2 200 points of an
// absolute 0.01 mm, for its side and its flat ends alike. Since mitcad#1275
// the tank's deviation by its size (the most, 0.5 mm, for its 24 m
// diagonal) is coarser still: about 310 points, a quarter of a millimetre
// off the circle of 5 m. Meshed on the model's thread first, the view
// keeps those meshes, the coarser edges included.
void checkTank(const occ::handle<Prs3d_Drawer>& drawer) {
  const TopoDS_Shape drawn = BRepPrimAPI_MakeCylinder(5000.0, 20000.0).Shape();
  CHECK(std::abs(mitcad::viewShapeDeviation(drawn) - mitcad::kViewDeviationMax) < 1e-9);
  CHECK(StdPrs_ToolTriangulatedShape::Tessellate(drawn, drawer));
  const int points = circlePoints(drawn);
  std::printf("Tank of 5 m radius in the view: %d points round its circles\n", points);
  CHECK(points > 250 && points <= 1200);

  const TopoDS_Shape shown = BRepPrimAPI_MakeCylinder(5000.0, 20000.0).Shape();
  CHECK(mitcad::io::mesh_in_place(shown, mitcad::viewMeshOptions()));
  const auto before = triangulations(shown);
  StdPrs_ToolTriangulatedShape::Tessellate(shown, drawer);
  CHECK(triangulations(shown) == before);
  CHECK(circlePoints(shown) == points);
}

// A wavy B-spline sheet `size` mm square: bicubic, poles x poles control
// points up and down `height` mm in turn (as geometry/io's test of the
// face bound).
TopoDS_Shape wavySheet(double size, int poles, double height) {
  NCollection_Array2<gp_Pnt> points(1, poles, 1, poles);
  for (int i = 1; i <= poles; ++i) {
    for (int j = 1; j <= poles; ++j) {
      points.SetValue(i, j,
                      gp_Pnt(size * (i - 1) / (poles - 1), size * (j - 1) / (poles - 1),
                             (i + j) % 2 == 0 ? height : -height));
    }
  }
  NCollection_Array1<double> knots(1, poles - 2);
  NCollection_Array1<int> multiplicities(1, poles - 2);
  for (int k = 1; k <= poles - 2; ++k) {
    knots.SetValue(k, static_cast<double>(k - 1));
    multiplicities.SetValue(k, k == 1 || k == poles - 2 ? 4 : 1);
  }
  const occ::handle<Geom_BSplineSurface> surface =
      new Geom_BSplineSurface(points, knots, knots, multiplicities, multiplicities, 3, 3);
  return BRepBuilderAPI_MakeFace(surface, Precision::Confusion()).Face();
}

// A face held within the face bound in the view (mitcad#1275): a wavy
// B-spline sheet of 60 mm (0.08 mm by its size, and still too small for
// the bound's floor), meshed by the view's drawer and on the model's thread,
// has at most kViewFaceTriangles triangles, the same both ways; the drawer
// keeps the model thread's, and finds them meshed finely enough (they
// record the drawers' deviation).
void checkHeldFace(const occ::handle<Prs3d_Drawer>& drawer) {
  const TopoDS_Shape drawn = wavySheet(60.0, 34, 3.0);
  CHECK(std::abs(mitcad::viewShapeDeviation(drawn) - 0.08) < 1e-9);
  CHECK(StdPrs_ToolTriangulatedShape::Tessellate(drawn, drawer));
  const std::vector<int> counts = triangleCounts(drawn);
  std::printf("Wavy sheet of 60 mm in the view: %d triangles\n", counts.empty() ? -1 : counts.front());
  CHECK(counts.size() == 1);
  CHECK(!counts.empty() && counts.front() > 0 && counts.front() <= static_cast<int>(mitcad::kViewFaceTriangles));

  const TopoDS_Shape shown = wavySheet(60.0, 34, 3.0);
  CHECK(mitcad::io::mesh_in_place(shown, mitcad::viewMeshOptions()));
  CHECK(triangleCounts(shown) == counts);
  const auto before = triangulations(shown);
  StdPrs_ToolTriangulatedShape::Tessellate(shown, drawer);
  CHECK(triangulations(shown) == before);
  CHECK(StdPrs_ToolTriangulatedShape::IsTessellated(shown, drawer));
}

// A face OCCT's mesher fails on (mitcad#589: meshing such a face again
// took without end, on the UI thread): a plane bounded by a wire that
// crosses itself. Meshed for display on the model's thread, it keeps no
// triangulation; once marked so (DisplayMeshes), the view does not mesh
// the body again. A body whose faces all got one is meshed by the view as
// before, marked or not.
void checkUnmeshable() {
  BRepBuilderAPI_MakePolygon bowTie(gp_Pnt(0, 0, 0), gp_Pnt(10, 10, 0), gp_Pnt(10, 0, 0), gp_Pnt(0, 10, 0), true);
  const TopoDS_Shape crossed = BRepBuilderAPI_MakeFace(gp_Pln(), bowTie.Wire(), true).Shape();
  CHECK(mitcad::io::mesh_all_in_place({crossed}, mitcad::viewMeshOptions()) == 1);
  CHECK(!mitcad::io::has_triangulation(crossed));
  const mitcad::geometry::Shape body(crossed);
  CHECK(mitcad::viewMeshes(body));
  body.set_display_meshed();
  CHECK(!mitcad::viewMeshes(body));

  const TopoDS_Shape box = BRepPrimAPI_MakeBox(10.0, 20.0, 30.0).Shape();
  CHECK(mitcad::io::mesh_all_in_place({box}, mitcad::viewMeshOptions()) == 1);
  const mitcad::geometry::Shape meshed(box);
  CHECK(mitcad::drawerMeshes(meshed));
  meshed.set_display_meshed();
  CHECK(mitcad::viewMeshes(meshed));
  // Its drawers do not mesh it again (mitcad#878), nor the one above.
  CHECK(!mitcad::drawerMeshes(meshed));
  CHECK(!mitcad::drawerMeshes(body));
}

// A body a job meshed for display to the end keeps its meshes in the
// view's drawers (mitcad#878): the 9 m dome's sphere meshes within the face
// bound, with a deviation coarser than the drawers' 0.01 mm, so a drawer
// that may mesh finds it untessellated and runs the mesher over the whole
// body again on the UI thread, only to keep every face (0.6-0.8 s for a
// design's body of 6 700 faces). A body no job meshed is still meshed by
// its drawers.
void checkJobMeshesKept(const occ::handle<Prs3d_Drawer>& drawer) {
  const TopoDS_Shape big = dome(1.0);
  CHECK(mitcad::io::mesh_all_in_place({big}, mitcad::viewMeshOptions()) == 1);
  CHECK(mitcad::io::has_triangulation(big));
  CHECK(!StdPrs_ToolTriangulatedShape::IsTessellated(big, drawer));
  const mitcad::geometry::Shape body(big);
  CHECK(mitcad::drawerMeshes(body));
  body.set_display_meshed();
  CHECK(!mitcad::drawerMeshes(body));
  const mitcad::geometry::Shape unmeshed(dome(1.0));
  CHECK(mitcad::drawerMeshes(unmeshed));
}

// Small circles (mitcad#879, Preferences > Display > Small circles): the
// view's meshes are Adaptive by default, a small cylinder's circles (0.02
// mm, at the view's 0.01 mm) in 12 pieces a turn, in Full detail in 60 or
// more; a tank's of 5 m alike in both. The choice switches the view's
// options and the drawers' meshing with them.
void checkSmallCircles() {
  CHECK(mitcad::viewSmallCircles() == mitcad::SmallCircles::Adaptive);
  CHECK(mitcad::viewMeshOptions().small_circle_segments == mitcad::kViewSmallCircleSegments);
  CHECK(!mitcad::setViewSmallCircles(mitcad::SmallCircles::Adaptive)); // as it was
  const auto meshed = [](double radius, double height) {
    const TopoDS_Shape shape = BRepPrimAPI_MakeCylinder(radius, height).Shape();
    CHECK(mitcad::io::mesh_in_place(shape, mitcad::viewMeshOptions()));
    return circlePoints(shape);
  };
  const int smallAdaptive = meshed(0.02, 0.08);
  const int tankAdaptive = meshed(5000.0, 20000.0);
  CHECK(mitcad::setViewSmallCircles(mitcad::SmallCircles::FullDetail));
  CHECK(mitcad::viewMeshOptions().small_circle_segments == 0);
  const int smallFull = meshed(0.02, 0.08);
  const int tankFull = meshed(5000.0, 20000.0);
  // The drawers' meshing follows the choice too.
  const occ::handle<Prs3d_Drawer> defaults = new Prs3d_Drawer();
  defaults->SetupOwnDefaults();
  mitcad::setViewTessellation(defaults);
  const occ::handle<Prs3d_Drawer> drawer = new Prs3d_Drawer();
  drawer->SetLink(defaults);
  const TopoDS_Shape drawnFull = BRepPrimAPI_MakeCylinder(0.02, 0.08).Shape();
  CHECK(StdPrs_ToolTriangulatedShape::Tessellate(drawnFull, drawer));
  CHECK(mitcad::setViewSmallCircles(mitcad::SmallCircles::Adaptive));
  const TopoDS_Shape drawnAdaptive = BRepPrimAPI_MakeCylinder(0.02, 0.08).Shape();
  CHECK(StdPrs_ToolTriangulatedShape::Tessellate(drawnAdaptive, drawer));
  std::printf("Small circles: cylinder of 0.02 mm %d points adaptive, %d in full detail (drawn %d, %d); tank of 5 m "
              "%d, %d\n",
              smallAdaptive, smallFull, circlePoints(drawnAdaptive), circlePoints(drawnFull), tankAdaptive, tankFull);
  CHECK(smallAdaptive == 13); // 12 pieces, the first point again at the end
  CHECK(smallFull >= 61);
  CHECK(circlePoints(drawnAdaptive) == smallAdaptive);
  CHECK(circlePoints(drawnFull) == smallFull);
  CHECK(tankAdaptive == tankFull);
}

// The rim's two edges (those of its flat ring at the top) rounded 0.25 mm.
TopoDS_Shape roundedRim(const TopoDS_Shape& shell) {
  BRepFilletAPI_MakeFillet rounding(shell);
  for (TopExp_Explorer face(shell, TopAbs_FACE); face.More(); face.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(face.Current()));
    if (surface.GetType() != GeomAbs_Plane) continue;
    if (surface.Value(surface.FirstUParameter(), surface.FirstVParameter()).Z() < 1.0) continue; // the outlet's end
    for (TopExp_Explorer edge(face.Current(), TopAbs_EDGE); edge.More(); edge.Next()) {
      rounding.Add(0.25, TopoDS::Edge(edge.Current()));
    }
  }
  return rounding.Shape();
}

// A small funnel (mitcad#1624): a tube of 10 mm and a cone of 30 degrees
// above it, shelled 1 mm inside with both ends open (`rounded`: its rim's
// two edges rounded 0.25 mm, as the reporter's design). Its rounded rim
// is about 21 mm across, so the view meshes it at 0.04 mm and the shell
// before the rounding at 0.028 mm (io::shape_deviation).
TopoDS_Shape funnel(bool rounded) {
  const double top = 5.0 + 10.0 * std::tan(30.0 * std::atan(1.0) / 45.0);
  const TopoDS_Shape tube = BRepPrimAPI_MakeCylinder(5.0, 15.0).Shape();
  const TopoDS_Shape cone = BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 15), gp::DZ()), 5.0, top, 10.0).Shape();
  ShapeUpgrade_UnifySameDomain unified(BRepAlgoAPI_Fuse(tube, cone).Shape());
  unified.Build();
  const TopoDS_Shape outer = unified.Shape();
  NCollection_List<TopoDS_Shape> open;
  for (TopExp_Explorer it(outer, TopAbs_FACE); it.More(); it.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(it.Current())).GetType() == GeomAbs_Plane) open.Append(it.Current());
  }
  // Joined by intersection, as the model's shell: the tube's and the cone's
  // insides meet in an edge, not a rounding.
  BRepOffsetAPI_MakeThickSolid shelled;
  shelled.MakeThickSolidByJoin(outer, open, -1.0, 1e-3, BRepOffset_Skin, false, false, GeomAbs_Intersection);
  if (!rounded) return shelled.Shape();
  return roundedRim(shelled.Shape());
}

// The largest distance of a face's triangles' centroids from its surface,
// and the most a triangle's normal turns from the surface's at its
// centroid, in degrees.
struct FaceFit {
  double off = 0.0;
  double turn = 0.0;
};
FaceFit faceFit(const TopoDS_Face& face) {
  FaceFit fit;
  TopLoc_Location location;
  const auto mesh = BRep_Tool::Triangulation(face, location);
  if (mesh.IsNull() || !mesh->HasUVNodes()) return fit;
  const BRepAdaptor_Surface surface(face, false);
  for (int t = 1; t <= mesh->NbTriangles(); ++t) {
    int n1 = 0, n2 = 0, n3 = 0;
    mesh->Triangle(t).Get(n1, n2, n3);
    const gp_Pnt a = mesh->Node(n1).Transformed(location.Transformation());
    const gp_Pnt b = mesh->Node(n2).Transformed(location.Transformation());
    const gp_Pnt c = mesh->Node(n3).Transformed(location.Transformation());
    const gp_XY uv = (mesh->UVNode(n1).XY() + mesh->UVNode(n2).XY() + mesh->UVNode(n3).XY()) / 3.0;
    gp_Pnt on;
    gp_Vec du, dv;
    surface.D1(uv.X(), uv.Y(), on, du, dv);
    fit.off = std::max(fit.off, on.Distance(gp_Pnt((a.XYZ() + b.XYZ() + c.XYZ()) / 3.0)));
    const gp_Vec facet = gp_Vec(a, b).Crossed(gp_Vec(a, c));
    const gp_Vec normal = du.Crossed(dv);
    if (facet.Magnitude() > 1e-12 && normal.Magnitude() > 1e-12) {
      const double angle = facet.Angle(normal) * 45.0 / std::atan(1.0);
      fit.turn = std::max(fit.turn, std::min(angle, 180.0 - angle));
    }
  }
  return fit;
}

// The most an edge's polygon on one of its faces' triangulations lies off
// its polygon on the other's (mm): 0 when the faces meet at the same nodes,
// more where a gap opens between them.
double edgeGap(const TopoDS_Shape& shape, int& edges) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  double gap = 0.0;
  edges = 0;
  for (int e = 1; e <= faces.Extent(); ++e) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(e));
    std::vector<std::vector<gp_Pnt>> sides;
    for (const TopoDS_Shape& face : faces.FindFromIndex(e)) {
      TopLoc_Location location;
      const auto mesh = BRep_Tool::Triangulation(TopoDS::Face(face), location);
      if (mesh.IsNull()) continue;
      const auto polygon = BRep_Tool::PolygonOnTriangulation(edge, mesh, location);
      if (polygon.IsNull()) continue;
      std::vector<gp_Pnt> points;
      for (int n = 1; n <= polygon->NbNodes(); ++n) {
        points.push_back(mesh->Node(polygon->Node(n)).Transformed(location.Transformation()));
      }
      sides.push_back(points);
    }
    if (sides.size() < 2) continue;
    ++edges;
    for (std::size_t s = 1; s < sides.size(); ++s) {
      for (const auto& [one, other] : {std::pair{&sides[0], &sides[s]}, std::pair{&sides[s], &sides[0]}}) {
        for (const gp_Pnt& p : *one) {
          double nearest = std::numeric_limits<double>::max();
          for (std::size_t k = 1; k < other->size(); ++k) {
            const gp_Pnt& q0 = (*other)[k - 1];
            const gp_Pnt& q1 = (*other)[k];
            const gp_Vec along(q0, q1);
            const double length = along.SquareMagnitude();
            const double t = length > 0.0 ? std::clamp(gp_Vec(q0, p).Dot(along) / length, 0.0, 1.0) : 0.0;
            nearest = std::min(nearest, p.Distance(q0.Translated(along * t)));
          }
          gap = std::max(gap, nearest);
        }
      }
    }
  }
  return gap;
}

// A small funnel's rounded rim and its outlet in the view (mitcad#1624).
// The rim's roundings are tori of 0.25 mm about circles of 10 mm. BRepMesh
// puts a torus's inside nodes on a grid of its edges' parameters (rows
// 30 degrees apart round the rounding: its seam is a small circle,
// mitcad#879), and the default's insertion left many of them out, its
// triangles spanning the whole 120 degrees of the outer rounding (0.12 mm
// off it at the body's 0.04 mm, normals 64 degrees off): the rim's outline
// showed saw teeth. Tori are triangulated through Delabella's base
// triangulation now (io's torus_algorithms). The faces meet at the same
// nodes on every edge, the body meshed as opened and meshed after the
// shell before it (at 0.028 mm) with the faces the rounding left as they
// were kept: the reporter's dark sliver at the outlet is the background
// seen through the open tube, not a gap between faces.
void checkFunnel() {
  const double deviation = mitcad::viewShapeDeviation(funnel(true));
  std::printf("Small funnel: deviation %g mm\n", deviation);
  CHECK(std::abs(deviation - 0.04) < 1e-9);
  const auto checkFaces = [deviation](const TopoDS_Shape& body, const char* how) {
    int tori = 0;
    for (TopExp_Explorer it(body, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      const GeomAbs_SurfaceType type = BRepAdaptor_Surface(face).GetType();
      if (type == GeomAbs_Plane) continue;
      const FaceFit fit = faceFit(face);
      TopLoc_Location location;
      const auto mesh = BRep_Tool::Triangulation(face, location);
      std::printf("  %s: %s face, %d triangles, off by %.3g mm, normals turned by %.3g degrees\n", how,
                  type == GeomAbs_Torus ? "torus" : type == GeomAbs_Cone ? "cone" : "cylinder",
                  mesh.IsNull() ? -1 : mesh->NbTriangles(), fit.off, fit.turn);
      CHECK(fit.off <= deviation * 1.05);
      if (type == GeomAbs_Torus) {
        ++tori;
        // No triangle across the rounding: its normal within 20 degrees of the surface's.
        CHECK(fit.turn <= 20.0);
      }
    }
    CHECK(tori == 2);
    int edges = 0;
    const double gap = edgeGap(body, edges);
    std::printf("  %s: %d edges between two faces, the faces' polygons apart by %.3g mm\n", how, edges, gap);
    CHECK(edges > 0);
    CHECK(gap < 1e-6);
  };
  const TopoDS_Shape opened = funnel(true);
  CHECK(mitcad::io::mesh_all_in_place({opened}, mitcad::viewMeshOptions()) == 1);
  checkFaces(opened, "opened");

  // Rounded in the view: the shell shown first, then its rounding, which
  // keeps the faces the rounding leaves as they were (as DisplayMeshes
  // meshes a command's result).
  const TopoDS_Shape shell = funnel(false);
  CHECK(mitcad::io::mesh_all_in_place({shell}, mitcad::viewMeshOptions()) == 1);
  const TopoDS_Shape rounded = roundedRim(shell);
  mitcad::io::reuse_triangulations(rounded, {shell});
  CHECK(mitcad::io::mesh_all_in_place({rounded}, mitcad::viewMeshOptions()) == 1);
  checkFaces(rounded, "rounded after the shell");

  // A whole torus of the same rounding (two seams).
  const TopoDS_Shape ring = BRepPrimAPI_MakeTorus(10.0, 0.25).Shape();
  const double ringDeviation = mitcad::viewShapeDeviation(ring);
  CHECK(mitcad::io::mesh_all_in_place({ring}, mitcad::viewMeshOptions()) == 1);
  for (TopExp_Explorer it(ring, TopAbs_FACE); it.More(); it.Next()) {
    const FaceFit fit = faceFit(TopoDS::Face(it.Current()));
    std::printf("  ring of 0.25 mm on 10 mm: off by %.3g mm at %.3g mm, normals turned by %.3g degrees\n", fit.off,
                ringDeviation, fit.turn);
    CHECK(fit.off > 0.0 && fit.off <= ringDeviation * 1.05);
    CHECK(fit.turn <= 20.0);
  }
}
} // namespace

int main(int argc, char** argv) {
  if (argc != 2) return 2;
  std::filesystem::create_directories(argv[1]);
  const std::string path = (std::filesystem::path(argv[1]) / "cylinder.step").string();
  // The application's meshing algorithm (OcctViewer does this).
  mitcad::useViewMeshing();
  try {
    checkSmallCircles();
    // The boundary's deviation under a pixel at 500 pixels' radius is Full
    // detail's: Adaptive cuts a small circle by the body's deviation alone
    // (mitcad#879, checkSmallCircles).
    CHECK(mitcad::setViewSmallCircles(mitcad::SmallCircles::FullDetail));
    checkCylinder(path, 0.02, 0.08);
    checkCylinder(path, 5.0, 20.0);
    checkCylinder(path, 5000.0, 20000.0);
    checkCylinder(path, 5.0, 50000.0);
    CHECK(mitcad::setViewSmallCircles(mitcad::SmallCircles::Adaptive));
    const occ::handle<Prs3d_Drawer> defaults = new Prs3d_Drawer();
    defaults->SetupOwnDefaults();
    mitcad::setViewTessellation(defaults);
    const occ::handle<Prs3d_Drawer> drawer = new Prs3d_Drawer();
    drawer->SetLink(defaults);
    checkDome(drawer);
    checkTank(drawer);
    checkHeldFace(drawer);
    checkUnmeshable();
    checkJobMeshesKept(drawer);
    checkFunnel();
  } catch (const Standard_Failure& error) {
    std::fprintf(stderr, "OCCT: %s\n", error.what());
    return 1;
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s\n", error.what());
    return 1;
  }
  return failures == 0 ? 0 : 1;
}
