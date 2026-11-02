// SPDX-License-Identifier: MIT
// Chamfers that broke OCCT's chamfer (TKFillet), on synthetic shapes: the
// case behind the patches 0090 and 0091 of the OCCT port
// (third_party/vcpkg-ports, mitcad#160).

#include <cmath>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom2dAdaptor_Curve.hxx>
#include <Geom2d_Circle.hxx>
#include <Geom_Circle.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/query.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kBore = 12.0;   // counterbore radius
constexpr double kHole = 8.5;    // through hole radius
constexpr double kFloor = 20.0;  // height of the counterbore's floor

// The face on a cylinder of another radius about the same axis, its edges
// keeping their p-curves (the same in the cylinder's parameters).
TopoDS_Shape with_radius(const TopoDS_Shape& body, const TopoDS_Face& face, double radius) {
  BRepAdaptor_Surface surface(face);
  BRep_Builder builder;
  TopoDS_Face fresh;
  builder.MakeFace(fresh, new Geom_CylindricalSurface(surface.Cylinder().Position(), radius),
                   BRep_Tool::Tolerance(face));
  const TopoDS_Face plain = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  for (TopoDS_Iterator wire(plain); wire.More(); wire.Next()) {
    builder.Add(fresh, wire.Value());
  }
  for (TopExp_Explorer e(plain, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    double first = 0.0;
    double last = 0.0;
    builder.UpdateEdge(edge, BRep_Tool::CurveOnSurface(edge, plain, first, last), fresh,
                       BRep_Tool::Tolerance(edge));
    builder.Range(edge, fresh, first, last);
  }
  fresh.Orientation(face.Orientation());
  BRepTools_ReShape reshape;
  reshape.Replace(face, fresh);
  return reshape.Apply(body);
}

// A block 60 x 60 x 40 with a through hole of radius 8.5 and, from height
// 20 up, a counterbore of radius 12: its floor is a ring 3.5 wide. The
// hole's circle on the floor has its vertex at 180 degrees; the
// counterbore's wall and its circle on the floor are in two pieces, split
// at 180 and 270 degrees. The wall's long piece (270 degrees) has a radius
// 1e-12 smaller, its short piece 1e-12 larger, as rounding leaves the
// pieces of a wall that other operations built: a chamfer of the floor's
// width runs a hair inside the hole along the long piece and a hair inside
// the floor along the short one, the two meeting at the hole's vertex.
TopoDS_Shape counterbored_block() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, 0), gp_Pnt(30, 30, 40)).Shape();
  const gp_Dir up(0, 0, 1);
  const gp_Dir at_180(-1, 0, 0);
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(block);
  tools.Append(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), up, at_180), kHole, 42).Shape());
  tools.Append(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kFloor), up, at_180), kBore, 21).Shape());
  cut.SetArguments(arguments);
  cut.SetTools(tools);
  cut.Build();
  // The wall split at 270 degrees.
  BRepAlgoAPI_Splitter splitter;
  NCollection_List<TopoDS_Shape> split;
  NCollection_List<TopoDS_Shape> line;
  split.Append(cut.Shape());
  line.Append(BRepBuilderAPI_MakeEdge(gp_Pnt(0, -kBore, kFloor), gp_Pnt(0, -kBore, 40)).Edge());
  splitter.SetArguments(split);
  splitter.SetTools(line);
  splitter.Build();
  TopoDS_Shape body = TopExp_Explorer(splitter.Shape(), TopAbs_SOLID).Current();
  // The wall's pieces a hair smaller and larger, with their circles on the
  // floor.
  for (const bool long_piece : {true, false}) {
    const double radius = long_piece ? kBore - 1e-12 : kBore + 1e-12;
    for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
      const TopoDS_Face& face = TopoDS::Face(f.Current());
      BRepAdaptor_Surface surface(face);
      if (surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - kBore) < 1e-9 &&
          (surface.LastUParameter() - surface.FirstUParameter() > kPi) == long_piece) {
        body = with_radius(body, face, radius);
        break;
      }
    }
  }
  TopoDS_Face floor;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Location().Z() - kFloor) < 1e-9) {
      floor = TopoDS::Face(f.Current());
    }
  }
  BRep_Builder builder;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() != GeomAbs_Cylinder || std::abs(surface.Cylinder().Radius() - kBore) > 1e-9) {
      continue;
    }
    const double radius = surface.Cylinder().Radius();
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      BRepAdaptor_Curve curve(edge);
      if (curve.GetType() != GeomAbs_Circle || std::abs(curve.Circle().Location().Z() - kFloor) > 1e-9) {
        continue;
      }
      const double tolerance = BRep_Tool::Tolerance(edge);
      builder.UpdateEdge(edge, new Geom_Circle(curve.Circle().Position(), radius), tolerance);
      double first = 0.0;
      double last = 0.0;
      const Geom2dAdaptor_Curve on_floor(BRep_Tool::CurveOnSurface(edge, floor, first, last));
      builder.UpdateEdge(edge, new Geom2d_Circle(on_floor.Circle().Position(), radius), floor, tolerance);
      builder.Range(edge, curve.FirstParameter(), curve.LastParameter());
    }
  }
  return body;
}

// The counterbore's long circle on the floor.
TopoDS_Edge long_arc(const TopoDS_Shape& body) {
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - kBore) < 1e-9 &&
        std::abs(curve.Circle().Location().Z() - kFloor) < 1e-9 &&
        curve.LastParameter() - curve.FirstParameter() > kPi) {
      return TopoDS::Edge(e.Current());
    }
  }
  return TopoDS_Edge();
}

// The counterbore's circle on the floor chamfered by the floor's width
// (patch 0090): the chamfer's two pieces meet on the hole's circle at its
// vertex, with opposite transitions, and ChFi3d_FilDS recorded the second
// one's interference on the hole's edge as a point with the vertex's
// index. TopOpeBRepBuild_Builder::BuildEdges counted it past the end of
// its array of points: the heap was corrupted and the process aborted
// ("free(): invalid next size"); a debug build of OCCT threw
// Standard_OutOfRange. The patched chamfer builds without either.
void test_chamfer_pieces_meeting_at_a_vertex_of_the_edge_they_reach() {
  const TopoDS_Shape body = counterbored_block();
  const TopoDS_Edge arc = long_arc(body);
  CHECK(!arc.IsNull());
  if (arc.IsNull()) {
    return;
  }
  BRepFilletAPI_MakeChamfer chamfer(body);
  chamfer.Add(kBore - kHole, arc);
  CHECK(chamfer.NbContours() == 1 && chamfer.NbEdges(1) == 2);
  bool failed = false;
  try {
    OCC_CATCH_SIGNALS
    chamfer.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "chamfer kernel test: %s\n", failure.what());
    failed = true;
  }
  CHECK(!failed);
}

// The same chamfer through Mitcad's: a valid body, or an error, never a
// corrupted process.
void test_chamfer_of_the_floor_width_through_mitcad() {
  const TopoDS_Shape body = counterbored_block();
  std::vector<Shape::NamedFace> faces;
  int index = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"F1:f" + std::to_string(index++)}});
  }
  const Shape shape(body, faces);
  const auto name = shape.name_of_edge(long_arc(body));
  CHECK(name.has_value());
  if (!name) {
    return;
  }
  ChamferSet set;
  set.edges = {*name};
  set.spec.distance = kBore - kHole;
  try {
    const ShapePtr chamfered = chamfer("F2", shape, {set});
    CHECK(BRepCheck_Analyzer(chamfered->occt()).IsValid());
    CHECK(volume(*chamfered) > volume(shape));
  } catch (const std::runtime_error& error) {
    std::fprintf(stderr, "chamfer kernel test (expected failure): %s\n", error.what());
  }
}


// Whether a shape is closed solids: a solid, every face in one, every shell
// of them closed.
bool closed_solids(const TopoDS_Shape& shape) {
  int solids = 0;
  int in_solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
    for (TopExp_Explorer face(solid.Current(), TopAbs_FACE); face.More(); face.Next()) {
      ++in_solids;
    }
  }
  int faces = 0;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    ++faces;
  }
  return solids > 0 && faces == in_solids;
}

// (mitcad#1182) A body's top rim bevelled 2 mm along its tangent chain:
// OCCT's chamfer leaves a gap where a bevel cut by an arc meets one cut by
// a chord between the same two vertices, and shape healing made that a
// valid open shell, which the chamfer returned: the body became a surface
// body without a word. A chamfer of a closed solid gives closed solids or
// fails, never an open body. The body is the case `chamfer-open-shell` of
// MITCAD_GEOMETRY_CASES (`body.brep`, a reported model's body before the
// chamfer); no synthetic layout reproduced OCCT's gap. (The design no longer
// makes this body: its sketch's region is found as saved, without the
// sliver.)
void test_chamfer_never_leaves_the_body_open() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_chamfer_never_leaves_the_body_open: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const ShapePtr body = case_body(cases + "/chamfer-open-shell/body.brep", "F1");
  CHECK(body != nullptr);
  if (body == nullptr) {
    return;
  }
  CHECK(closed_solids(body->occt()));
  // The rim's arc of radius 3.7 at a corner of the top face.
  std::string edge;
  for (int e = 0; e < body->edge_count() && edge.empty(); ++e) {
    const BRepAdaptor_Curve curve(body->edge(e));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - 3.7) < 1.0e-6 &&
        curve.Circle().Location().Distance(gp_Pnt(-19.0, -7.870057685088811, 56.0)) < 1.0e-6) {
      edge = body->edge_name(e);
    }
  }
  CHECK(!edge.empty());
  if (edge.empty()) {
    return;
  }
  ChamferSet set;
  set.edges = {edge};
  set.spec.distance = 2.0;
  // At its exact size the bevel takes a sliver face of the rim away: built
  // 0.1 % smaller, it is closed.
  const ShapePtr chamfered = chamfer("F2", *body, {set});
  CHECK(closed_solids(chamfered->occt()));
  CHECK(BRepCheck_Analyzer(chamfered->occt()).IsValid());
  const double before = volume(*body);
  const double after = volume(*chamfered);
  CHECK(after < before && after > 0.95 * before);
}

} // namespace

void chamfer_kernel_tests() {
  guarded("test_chamfer_pieces_meeting_at_a_vertex_of_the_edge_they_reach",
          test_chamfer_pieces_meeting_at_a_vertex_of_the_edge_they_reach);
  guarded("test_chamfer_of_the_floor_width_through_mitcad", test_chamfer_of_the_floor_width_through_mitcad);
  guarded("test_chamfer_never_leaves_the_body_open", test_chamfer_never_leaves_the_body_open);
}

} // namespace test
