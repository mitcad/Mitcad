// SPDX-License-Identifier: MIT
// Fillets of a short slanted corner edge where a thin wall meets an
// overhanging end band whose top vertex lies on an earlier rounding
// (mitcad#1253, mitcad#1256).
//
// The edge runs from the bottom up to where the rounding cuts the wall and
// the band, a vertex of three convex edges, the rounding the face at the
// end there. Past that vertex OCCT walks the blend on until both contacts
// have left their faces across the rounding's edges, and the corner cuts
// the blend with the rounding between the two points. With a radius larger
// than about half the edge's length the contact on the band meets the
// rounding farther past the vertex than the spine's extension reached: the
// walk stopped with that contact inside the band, and OCCT built a corner
// folded back on itself (0.75) or an invalid one (1). The port's patch 1240
// walks such an end again over a longer extension: every radius builds one
// blend face. A made face that reaches out of the body, where a fillet of
// convex edges only takes material away, gets a warning naming it (never a
// refusal); none does here.
//
// The reported part (MITCAD_STEP_REPORTED, a folder of reported STEP
// files, never committed; skipped without it) has two such corners, at
// each end of its wall. A synthetic body has the same corner, made of
// planes and a cylinder, its faces converted to B-spline surfaces as the
// STEP import gives them. The check itself is tested on made faces placed
// by hand.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <filesystem>
#include <memory>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "../src/dressup_check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/io/step.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

std::string env(const char* name) {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, name) == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? std::string(value) : std::string();
#endif
}

bool contains(const std::string& text, const std::string& part) { return text.find(part) != std::string::npos; }

std::string notes(const Shape& shape) {
  std::string text;
  for (const std::string& note : shape.notes()) {
    text += (text.empty() ? "" : "; ") + note;
  }
  return text;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    ++n;
  }
  return n;
}

FilletSet edge_set(const std::string& edge, double radius) {
  FilletSet set;
  set.edges = {edge};
  set.radius = radius;
  return set;
}

// The fillet, or null with its error printed.
ShapePtr rounded(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets) {
  try {
    return fillet(feature, body, sets);
  } catch (const std::exception& failure) {
    std::printf("  %s\n", failure.what());
    return nullptr;
  }
}

// A fillet of a corner edge that built clean: one blend face more than the
// body, valid, no warning; prints what it removed.
void check_clean(const char* label, const Shape& body, const ShapePtr& result, double radius) {
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  const double removed = volume_of(body.occt()) - volume_of(result->occt());
  std::printf("  %s %.2f: removed %.6f, %s\n", label, radius, removed, notes(*result).c_str());
  CHECK(result->notes().empty());
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(removed > 0.0);
}

// The reported part's first body, its faces named as the model's import
// names them (F1:import(0), ...); null without the file.
ShapePtr reported_part() {
  const std::string folder = env("MITCAD_STEP_REPORTED");
  if (folder.empty() || !std::filesystem::is_directory(folder)) {
    return nullptr;
  }
  for (const auto& entry : std::filesystem::directory_iterator(folder)) {
    if (entry.path().filename().string().rfind("github-130-", 0) == 0) {
      const auto bodies = mitcad::io::read_step(entry.path().string());
      if (!bodies.empty()) {
        return import_body(bodies.front().shape, "F1", 0);
      }
    }
  }
  return nullptr;
}

void test_reported_part() {
  const ShapePtr part = reported_part();
  if (!part) {
    std::printf("test_reported_part: skipped (MITCAD_STEP_REPORTED has no reported part)\n");
    return;
  }
  // The corner edge (0.84 long) and its mirror at the wall's other end.
  const std::string corner = "E{F1:import(24)|F1:import(4)}";
  const std::string mirror = "E{F1:import(26)|F1:import(5)}";
  CHECK(part->find_edges(corner).size() == 1);
  CHECK(part->find_edges(mirror).size() == 1);

  // Every radius: one blend face, clean, removing more with the radius
  // (before patch 1240: 0.75 folded at the top vertex with the warning, 1
  // invalid).
  for (const std::string& edge : {corner, mirror}) {
    double before = 0.0;
    for (const double radius : {0.5, 0.75, 1.0}) {
      const ShapePtr result = rounded("F2", *part, {edge_set(edge, radius)});
      check_clean("corner", *part, result, radius);
      if (result) {
        CHECK(result->face_count() == part->face_count() + 1);
        const double removed = volume_of(part->occt()) - volume_of(result->occt());
        CHECK(removed > before);
        before = removed;
      }
    }
  }

  // The bottom rounded first (1 mm, clean), then what is left of the corner
  // edge: clean at both corners (before: spikes 0.16 and 0.77 out of the
  // body at 1).
  FilletSet bottom;
  bottom.faces = {"F1:import(28)"};
  bottom.radius = 1.0;
  const ShapePtr step = rounded("F2", *part, {bottom});
  CHECK(step && step->notes().empty());
  if (!step) {
    return;
  }
  for (const std::string& edge : {corner, mirror}) {
    CHECK(step->find_edges(edge).size() == 1);
    for (const double radius : {0.5, 0.75, 1.0}) {
      check_clean("sliver", *step, rounded("F3", *step, {edge_set(edge, radius)}), radius);
    }
  }
}

// The edge of a shape whose ends are near two points, or a null edge.
TopoDS_Edge edge_between(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b, double within) {
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt p = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt q = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if ((p.Distance(a) < within && q.Distance(b) < within) || (p.Distance(b) < within && q.Distance(a) < within)) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// The reported corner made of planes and a cylinder: a wall of 1.44 along
// y = 0 under a slope rising toward -y (1.0357 per unit, as the part's),
// ended at x = 0 by a vertical end wall, the slope's edge with the end wall
// rounded with 2 (OCCT's fillet: a cylinder), then the end undercut by a
// band leaning 0.55 (away x > -1.1 + 0.55 z, below z = 2), which cuts the
// rounding. The corner edge where the band meets the wall runs from
// (-1.1, 0, 0) up to the vertex where the rounding cuts both, at
// (-0.693, 0, 0.740) as on the part. With `nurbs`, its faces are B-spline
// surfaces, as the STEP import gives them, so OCCT walks the blend there
// instead of building a cylinder.
TopoDS_Shape synthetic_body(bool nurbs) {
  const double slope = 1.0357;
  const double depth = 10.0;
  BRepBuilderAPI_MakePolygon section(gp_Pnt(-20.0, 0.0, 0.0), gp_Pnt(-20.0, 0.0, 1.44),
                                     gp_Pnt(-20.0, -depth, 1.44 + slope * depth), gp_Pnt(-20.0, -depth, 0.0), true);
  const TopoDS_Shape wedge =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(section.Wire()).Face(), gp_Vec(20.0, 0.0, 0.0)).Shape();
  const TopoDS_Edge ridge =
      edge_between(wedge, gp_Pnt(0.0, 0.0, 1.44), gp_Pnt(0.0, -depth, 1.44 + slope * depth), 1.0e-7);
  CHECK(!ridge.IsNull());
  if (ridge.IsNull()) {
    return TopoDS_Shape();
  }
  BRepFilletAPI_MakeFillet round(wedge);
  round.Add(2.0, ridge);
  round.Build();
  CHECK(round.IsDone());
  if (!round.IsDone()) {
    return TopoDS_Shape();
  }
  BRepBuilderAPI_MakePolygon away(gp_Pnt(-1.1 - 0.55, 5.0, -1.0), gp_Pnt(-1.1 + 0.55 * 3.0, 5.0, 3.0),
                                  gp_Pnt(5.0, 5.0, 3.0), gp_Pnt(5.0, 5.0, -1.0), true);
  const TopoDS_Shape cutter =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(away.Wire()).Face(), gp_Vec(0.0, -depth - 10.0, 0.0)).Shape();
  BRepAlgoAPI_Cut cut(round.Shape(), cutter);
  CHECK(cut.IsDone());
  if (!cut.IsDone()) {
    return TopoDS_Shape();
  }
  return nurbs ? BRepBuilderAPI_NurbsConvert(cut.Shape(), true).Shape() : cut.Shape();
}

// The synthetic corner edge, from the bottom up to the rounding.
TopoDS_Edge synthetic_corner(const TopoDS_Shape& body) {
  return edge_between(body, gp_Pnt(-1.1, 0.0, 0.0), gp_Pnt(-0.692966, 0.0, 0.740062), 1.0e-5);
}

// OCCT alone: the corner edge of the B-spline body rounded with 0.5, 0.75
// and 1 builds one blend face, valid, removing what the same rounding
// removes from the body of planes and a cylinder, where OCCT builds the
// blend as a cylinder up to the rounding (exact). Before patch 1240, 0.75
// and 1 built a corner face folded back on itself, 0.32 out of the body,
// and an invalid shape.
void test_synthetic_corner_occt() {
  const TopoDS_Shape exact = synthetic_body(false);
  const TopoDS_Shape walked = synthetic_body(true);
  CHECK(!exact.IsNull() && !walked.IsNull());
  if (exact.IsNull() || walked.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(walked).IsValid());
  const TopoDS_Edge e_exact = synthetic_corner(exact);
  const TopoDS_Edge e_walked = synthetic_corner(walked);
  CHECK(!e_exact.IsNull() && !e_walked.IsNull());
  if (e_exact.IsNull() || e_walked.IsNull()) {
    return;
  }
  for (const double radius : {0.5, 0.75, 1.0}) {
    BRepFilletAPI_MakeFillet a(exact), b(walked);
    a.Add(radius, e_exact);
    b.Add(radius, e_walked);
    try {
      OCC_CATCH_SIGNALS
      a.Build();
      b.Build();
    } catch (const Standard_Failure& failure) {
      std::printf("  synthetic %.2f: %s\n", radius, failure.what());
    }
    CHECK(a.IsDone());
    CHECK(b.IsDone());
    if (!a.IsDone() || !b.IsDone()) {
      continue;
    }
    const double removed_exact = volume_of(exact) - volume_of(a.Shape());
    const double removed = volume_of(walked) - volume_of(b.Shape());
    std::printf("  synthetic %.2f: removed %.7f (exact %.7f)\n", radius, removed, removed_exact);
    CHECK(BRepCheck_Analyzer(b.Shape()).IsValid());
    CHECK(face_count(b.Shape()) == face_count(walked) + 1);
    // (The walked blend approximates the cylinder within OCCT's
    // approximation tolerances, and the B-spline body's volume is
    // integrated as such.)
    CHECK(std::abs(removed - removed_exact) <= 5.0e-5);
  }
}

// The same through MitCad's fillet: clean at every radius, also on what is
// left of the corner edge after the bottom is rounded with 1.
void test_synthetic_corner() {
  const TopoDS_Shape walked = synthetic_body(true);
  CHECK(!walked.IsNull());
  if (walked.IsNull()) {
    return;
  }
  const ShapePtr part = import_body(walked, "F1", 0);
  const int corner = part->edge_index(synthetic_corner(walked));
  CHECK(corner >= 0);
  if (corner < 0) {
    return;
  }
  const std::string edge = part->edge_name(corner);
  for (const double radius : {0.5, 0.75, 1.0}) {
    const ShapePtr result = rounded("F2", *part, {edge_set(edge, radius)});
    check_clean("synthetic", *part, result, radius);
    CHECK(result && result->face_count() == part->face_count() + 1);
  }
  int bottom = -1;
  for (int f = 0; f < part->face_count() && bottom < 0; ++f) {
    bool flat = true;
    for (TopExp_Explorer v(part->face(f), TopAbs_VERTEX); v.More(); v.Next()) {
      flat = flat && std::abs(BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Z()) < 1.0e-6;
    }
    bottom = flat ? f : -1;
  }
  CHECK(bottom >= 0);
  if (bottom < 0) {
    return;
  }
  FilletSet floor;
  floor.faces = {part->face_names(bottom).front()};
  floor.radius = 1.0;
  const ShapePtr step = rounded("F2", *part, {floor});
  CHECK(step && step->notes().empty());
  if (!step || step->find_edges(edge).size() != 1) {
    CHECK(step && step->find_edges(edge).size() == 1);
    return;
  }
  for (const double radius : {0.5, 0.75, 1.0}) {
    check_clean("synthetic sliver", *step, rounded("F3", *step, {edge_set(edge, radius)}), radius);
  }
}

// The check on made faces placed by hand: a box, and the box with a block
// on its top named as a fillet's corner (out of the body) or with a pocket
// in it named as a fillet of a concave edge (into the body).
void test_check_on_placed_faces() {
  using detail::DressupSide;
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), gp_Pnt(10.0, 10.0, 10.0)).Shape();
  const ShapePtr input = import_body(box, "F1", 0);
  const auto named = [](const TopoDS_Shape& shape, const gp_Pnt& near) {
    // The faces not of the box (within 1e-6 of its faces' planes) named as
    // the fillet's corner and its edge, the others as the box's.
    std::vector<Shape::NamedFace> faces;
    int k = 0;
    for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next(), ++k) {
      bool of_box = true;
      for (TopExp_Explorer v(f.Current(), TopAbs_VERTEX); v.More(); v.Next()) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
        of_box = of_box && (p.Distance(near) > 3.0);
      }
      faces.push_back({f.Current(), {of_box ? "F1:import(" + std::to_string(k) + ")"
                                            : "F2:corner(V{F1:import(1)|F1:import(3)|F1:import(5)})"}});
    }
    return std::make_shared<Shape>(shape, faces);
  };
  // A block of 1 standing 0.5 out of the top: reaches 0.5 out of the body.
  const TopoDS_Shape bump =
      BRepAlgoAPI_Fuse(box, BRepPrimAPI_MakeBox(gp_Pnt(4.0, 4.0, 9.5), gp_Pnt(5.0, 5.0, 10.5)).Shape()).Shape();
  const ShapePtr out = named(bump, gp_Pnt(4.5, 4.5, 10.5));
  const std::string warning = detail::dressup_warning(*input, *out, "F2", "fillet", DressupSide::Within, 2.0);
  std::printf("  bump: %s\n", warning.c_str());
  // In plain words (mitcad#1327): the corner by its place, not its name.
  CHECK(contains(warning, "The corner of the fillet reaches 0.5 mm out of the body, where a fillet of convex edges "
                          "only takes material away: it folds back on itself or ends in a spike there"));
  CHECK(contains(warning, "Check the result."));
  CHECK(!contains(warning, "V{") && !contains(warning, "E{") && !contains(warning, "F1:import("));
  // Its places: the corner's face and the deepest point, on the block.
  const detail::DressupFinding found =
      detail::dressup_check(*input, *out, "F2", "fillet", DressupSide::Within, 2.0);
  CHECK(found.text == warning);
  CHECK(found.places.size() == 2);
  if (found.places.size() == 2) {
    CHECK(found.places[0].kind == Shape::NotePlace::Kind::Face &&
          found.places[0].name == "F2:corner(V{F1:import(1)|F1:import(3)|F1:import(5)})");
    CHECK(found.places[1].kind == Shape::NotePlace::Kind::Point);
    const auto& p = found.places[1].point;
    CHECK(p[0] > 4.0 - 1e-6 && p[0] < 5.0 + 1e-6 && p[1] > 4.0 - 1e-6 && p[1] < 5.0 + 1e-6 && p[2] > 10.0);
  }
  // The same block's faces where material may be added: no warning.
  CHECK(detail::dressup_warning(*input, *out, "F2", "fillet", DressupSide::Outside, 2.0).empty());
  // Within 3 % of the size: no warning (mitcad#1327). 0.5 of 100, and of
  // 26 (1.9 %, as a three-fillet corner's 0.057 mm on a radius of 3); of
  // 10.6 (4.7 %, as the folded corner of mitcad#1253, 0.035 mm on 0.75) it
  // warns.
  CHECK(detail::dressup_warning(*input, *out, "F2", "fillet", DressupSide::Within, 100.0).empty());
  CHECK(detail::dressup_warning(*input, *out, "F2", "fillet", DressupSide::Within, 26.0).empty());
  CHECK(!detail::dressup_warning(*input, *out, "F2", "fillet", DressupSide::Within, 10.6).empty());
  // A pocket 0.5 deep in the top, where material may only be added.
  const TopoDS_Shape pocket =
      BRepAlgoAPI_Cut(box, BRepPrimAPI_MakeBox(gp_Pnt(4.0, 4.0, 9.5), gp_Pnt(5.0, 5.0, 10.5)).Shape()).Shape();
  const ShapePtr in = named(pocket, gp_Pnt(4.5, 4.5, 9.5));
  const std::string inward = detail::dressup_warning(*input, *in, "F2", "fillet", DressupSide::Outside, 2.0);
  std::printf("  pocket: %s\n", inward.c_str());
  CHECK(contains(inward, "reaches 0.5 mm into the body"));
  CHECK(detail::dressup_warning(*input, *in, "F2", "fillet", DressupSide::Within, 2.0).empty());
  // Without made faces: nothing.
  CHECK(detail::dressup_warning(*input, *input, "F2", "fillet", DressupSide::Within, 2.0).empty());
  // The side of a box's edge: within (convex).
  std::vector<int> owner(static_cast<std::size_t>(input->edge_count()), -1);
  owner[0] = 0;
  const auto side = detail::dressup_side(*input, owner);
  CHECK(side && *side == DressupSide::Within);
  // The corner's name in a message without the word "vertex".
  CHECK(detail::made_face("F2:corner(V{a|b|c})", "F2", "fillet", false) == "the corner of the fillet at V{a|b|c}");
  CHECK(detail::made_face("F2:fillet(E{a|b})#1", "F2", "fillet", true) == "the fillet of edge E{a|b}");
  CHECK(detail::made_face("F1:import(3)", "F2", "fillet", true) == "face F1:import(3)");
}

// The plane face of a named shape at `value` along axis `axis` (0 x, 1 y,
// 2 z), its normal along that axis; -1 for none.
int plane_face_at(const Shape& shape, int axis, double value) {
  for (int f = 0; f < shape.face_count(); ++f) {
    const BRepAdaptor_Surface surface(shape.face(f));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    const gp_Dir n = plane.Axis().Direction();
    const gp_Pnt o = plane.Location();
    const double normal[3] = {n.X(), n.Y(), n.Z()};
    const double at[3] = {o.X(), o.Y(), o.Z()};
    if (std::abs(std::abs(normal[axis]) - 1.0) < 1e-9 && std::abs(at[axis] - value) < 1e-6) {
      return f;
    }
  }
  return -1;
}

// The name of the edge between two faces of a named shape; empty for none.
std::string edge_between(const Shape& shape, int a, int b) {
  for (int e = 0; e < shape.edge_count(); ++e) {
    const std::array<int, 2> faces = shape.edge_faces(e);
    if ((faces[0] == a && faces[1] == b) || (faces[0] == b && faces[1] == a)) {
      return shape.edge_name(e);
    }
  }
  return {};
}

// An L of a floor [0, 20] x [0, 20] x [0, 10] and a wall [0, 20] x [0, 5] x
// [10, 20] on it, the floor's top edge at y = 20 rounded `radius` (none: 0).
TopoDS_Shape floor_and_wall(double radius) {
  const TopoDS_Shape floor = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 10)).Shape();
  const TopoDS_Shape wall = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 10), gp_Pnt(20, 5, 20)).Shape();
  BRepAlgoAPI_Fuse fused(floor, wall);
  fused.SimplifyResult();
  const TopoDS_Shape both = fused.Shape();
  if (!(radius > 0.0)) {
    return both;
  }
  BRepFilletAPI_MakeFillet rounding(both);
  for (TopExp_Explorer e(both, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.Y() - 20) < 1e-9 && std::abs(b.Y() - 20) < 1e-9 && std::abs(a.Z() - 10) < 1e-9 &&
        std::abs(b.Z() - 10) < 1e-9) {
      rounding.Add(radius, edge);
    }
  }
  rounding.Build();
  return rounding.IsDone() ? rounding.Shape() : TopoDS_Shape();
}

// The result of a fillet of the L's concave edge (between the floor's top
// and the wall's front) placed by hand: the L joined with a block whose
// faces outside the L are named as the fillet's made faces.
ShapePtr placed_fillet(const Shape& input, const std::string& edge, const gp_Pnt& low, const gp_Pnt& high) {
  const TopoDS_Shape joined = BRepAlgoAPI_Fuse(input.occt(), BRepPrimAPI_MakeBox(low, high).Shape()).Shape();
  std::vector<Shape::NamedFace> faces;
  for (TopExp_Explorer f(joined, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const gp_Pnt mid = BRepAdaptor_Surface(face).Value(0.5 * (u0 + u1), 0.5 * (v0 + v1));
    NameList names = {"F2:fillet(" + edge + ")"};
    for (int k = 0; k < input.face_count(); ++k) {
      BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(mid).Vertex(), input.face(k));
      if (distance.IsDone() && distance.Value() < 1e-6) {
        names = input.face_names(k);
        break;
      }
    }
    faces.push_back({face, names});
  }
  return std::make_shared<Shape>(joined, faces);
}

// A fillet of a concave edge whose made face runs on from the face beside
// the edge onto the rounding of that face's far edge (mitcad#1327: at a
// three-fillet corner, a tangent chain's fillet rests on the earlier
// rounding there, 0.057 mm off the face's surface continued, and warned).
// Placed by hand: a block above the rounding, 0.08 mm off the floor's top
// continued but resting on the rounding, gives no warning; the same block
// hanging past the floor's sharp edge (0.3 mm) warns with its places.
void test_check_runs_onto_a_rounding() {
  using detail::DressupSide;
  const TopoDS_Shape rounded_l = floor_and_wall(1.0);
  CHECK(!rounded_l.IsNull());
  if (rounded_l.IsNull()) {
    return;
  }
  const ShapePtr input = import_body(rounded_l, "F1", 0);
  const std::string edge = edge_between(*input, plane_face_at(*input, 2, 10.0), plane_face_at(*input, 1, 5.0));
  CHECK(!edge.empty());
  const ShapePtr over_rounding = placed_fillet(*input, edge, gp_Pnt(8, 19.2, 9.95), gp_Pnt(12, 19.6, 10.3));
  const detail::DressupFinding none =
      detail::dressup_check(*input, *over_rounding, "F2", "fillet", DressupSide::Outside, 2.0);
  std::printf("  over the rounding: %s\n", none.text.c_str());
  CHECK(none.empty());

  const ShapePtr sharp = import_body(floor_and_wall(0.0), "F1", 0);
  const std::string sharp_edge = edge_between(*sharp, plane_face_at(*sharp, 2, 10.0), plane_face_at(*sharp, 1, 5.0));
  CHECK(!sharp_edge.empty());
  const ShapePtr hanging = placed_fillet(*sharp, sharp_edge, gp_Pnt(8, 19.8, 9.9), gp_Pnt(12, 20.3, 10.3));
  const detail::DressupFinding past =
      detail::dressup_check(*sharp, *hanging, "F2", "fillet", DressupSide::Outside, 2.0);
  std::printf("  past the sharp edge: %s\n", past.text.c_str());
  CHECK(contains(past.text, "The fillet hangs 0.3 mm past the edge of the face beside it"));
  CHECK(!contains(past.text, "E{") && !contains(past.text, "F1:import("));
  // The made face, the floor's top it hangs past, and the farthest point.
  const std::string top = sharp->face_names(plane_face_at(*sharp, 2, 10.0)).front();
  CHECK(std::any_of(past.places.begin(), past.places.end(), [&](const Shape::NotePlace& place) {
    return place.kind == Shape::NotePlace::Kind::Face && place.name == "F2:fillet(" + sharp_edge + ")";
  }));
  CHECK(std::any_of(past.places.begin(), past.places.end(), [&](const Shape::NotePlace& place) {
    return place.kind == Shape::NotePlace::Kind::Face && place.name == top;
  }));
  CHECK(std::any_of(past.places.begin(), past.places.end(), [](const Shape::NotePlace& place) {
    return place.kind == Shape::NotePlace::Kind::Point && place.point[1] > 20.0;
  }));
}

// A synthetic three-fillet corner built by the kernel (mitcad#1327): a
// tower standing on a slab, its front edges rounded 5 (Fillet1) and the
// slab's top back edge rounded 1 (Fillet2), then the tower's foot, the
// concave edge chain between the slab's top and the tower (along its front
// and round its rounded edges, down its sides to the slab's back), rounded
// 3 with tangent chains: where it ends at the back, it runs from the
// slab's top onto the rounding of 1. The result has no warning. (Skipped
// with a message where the kernel cannot build it: the check, not the
// fillet, is what is tested.)
void test_three_fillet_corner() {
  const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(-50, 0, 0), gp_Pnt(50, 20, 20)).Shape();
  const TopoDS_Shape tower = BRepPrimAPI_MakeBox(gp_Pnt(-20, 0, 20), gp_Pnt(20, 10, 60)).Shape();
  BRepAlgoAPI_Fuse fused(slab, tower);
  fused.SimplifyResult();
  const TopoDS_Shape both = fused.Shape();
  BRepFilletAPI_MakeFillet first(both);
  for (TopExp_Explorer e(both, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    // The tower's front vertical edges (x = +-20, y = 10).
    if (std::abs(a.Y() - 10) < 1e-9 && std::abs(b.Y() - 10) < 1e-9 && std::abs(std::abs(a.X()) - 20) < 1e-9 &&
        std::abs(a.X() - b.X()) < 1e-9 && a.Z() > 19.0 && b.Z() > 19.0) {
      first.Add(5.0, edge);
    }
  }
  first.Build();
  if (!first.IsDone()) {
    std::printf("test_three_fillet_corner: skipped (the tower's roundings of 5 failed)\n");
    return;
  }
  const TopoDS_Shape towered = first.Shape();
  BRepFilletAPI_MakeFillet back(towered);
  for (TopExp_Explorer e(towered, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    // The slab's top back edges beside the tower (y = 0, z = 20).
    if (std::abs(a.Y()) < 1e-9 && std::abs(b.Y()) < 1e-9 && std::abs(a.Z() - 20) < 1e-9 &&
        std::abs(b.Z() - 20) < 1e-9) {
      back.Add(1.0, edge);
    }
  }
  back.Build();
  if (!back.IsDone()) {
    std::printf("test_three_fillet_corner: skipped (the slab's roundings of 1 failed)\n");
    return;
  }
  const ShapePtr body = import_body(back.Shape(), "F1", 0);
  // The foot's edge along the tower's front: between the slab's top (z = 20)
  // and the tower's front (y = 10).
  const std::string foot = edge_between(*body, plane_face_at(*body, 2, 20.0), plane_face_at(*body, 1, 10.0));
  CHECK(!foot.empty());
  if (foot.empty()) {
    return;
  }
  FilletSet set;
  set.edges = {foot};
  set.radius = 3.0;
  set.tangent_chain = true;
  const ShapePtr result = rounded("F4", *body, {set});
  if (!result) {
    std::printf("test_three_fillet_corner: skipped (the foot's rounding of 3 failed)\n");
    return;
  }
  std::printf("  three-fillet corner: %s\n", notes(*result).c_str());
  CHECK(result->notes().empty());
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_check_on_placed_faces", test::test_check_on_placed_faces);
  test::guarded("test_check_runs_onto_a_rounding", test::test_check_runs_onto_a_rounding);
  test::guarded("test_three_fillet_corner", test::test_three_fillet_corner);
  test::guarded("test_synthetic_corner_occt", test::test_synthetic_corner_occt);
  test::guarded("test_synthetic_corner", test::test_synthetic_corner);
  test::guarded("test_reported_part", test::test_reported_part);
  if (test::failures != 0) {
    std::printf("%d check(s) failed\n", test::failures);
  }
  return test::failures == 0 ? 0 : 1;
}
