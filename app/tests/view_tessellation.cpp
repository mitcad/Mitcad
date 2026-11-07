// SPDX-License-Identifier: MIT
// Exercise the viewport's OCCT meshing path on synthetic STEP round trips.
#include "view/ViewTessellation.hpp"
#include "mitcad/io/step.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <filesystem>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <gp_Pln.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
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
#include <StdPrs_ToolTriangulatedShape.hxx>
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
  } catch (const Standard_Failure& error) {
    std::fprintf(stderr, "OCCT: %s\n", error.what());
    return 1;
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s\n", error.what());
    return 1;
  }
  return failures == 0 ? 0 : 1;
}
