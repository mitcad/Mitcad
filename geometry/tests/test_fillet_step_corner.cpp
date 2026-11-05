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

#include <cmath>
#include <cstdio>
#include <filesystem>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
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
  CHECK(contains(warning, "the corner of the fillet at vertex V{F1:import(1)|F1:import(3)|F1:import(5)} reaches 0.5 mm "
                          "out of the body"));
  // The same block's faces where material may be added: no warning.
  CHECK(detail::dressup_warning(*input, *out, "F2", "fillet", DressupSide::Outside, 2.0).empty());
  // Within 1 % of the size: no warning (0.5 of 100).
  CHECK(detail::dressup_warning(*input, *out, "F2", "fillet", DressupSide::Within, 100.0).empty());
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

} // namespace
} // namespace test

int main() {
  test::guarded("test_check_on_placed_faces", test::test_check_on_placed_faces);
  test::guarded("test_synthetic_corner_occt", test::test_synthetic_corner_occt);
  test::guarded("test_synthetic_corner", test::test_synthetic_corner);
  test::guarded("test_reported_part", test::test_reported_part);
  if (test::failures != 0) {
    std::printf("%d check(s) failed\n", test::failures);
  }
  return test::failures == 0 ? 0 : 1;
}
