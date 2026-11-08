// SPDX-License-Identifier: MIT
// A block standing on a plate, its foot edges rounded (mitcad#1458, from
// GitHub mitcad/Mitcad#177).
//
// Three foot edges of the block (the concave edges between its sides and
// the plate's top: its front and the two ends, the front meeting each end
// at a vertical convex corner of the block) rounded with 4 mm: each end's
// rounding runs on past the corner, out of the corner between the end and
// the plate, until it meets the front's along the mitre that continues the
// corner's edge down to the plate. The concave check (dressup_check.hpp,
// mitcad#1260) took that for a rounding hanging out past the end's face
// ("The fillet hangs 0.72 mm past the edge of the face beside it"): the
// mitre fills the corner outside the convex edge as the roundings fill
// the corners beside it, and hangs out past nothing. No warning.
//
// The ends alone rounded: the front's foot edge then runs on between the
// roundings, cut short or in pieces numbered anew, and a click on it in
// the preview names it so. item_origin (item_origin.hpp) finds the foot
// edge of the block before the fillet for every piece; the roundings'
// faces and edges are the fillet's own, with no origin before it.

#include <algorithm>
#include <array>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "../src/dressup_check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/item_origin.hpp"
#include "mitcad/geometry/naming.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kRadius = 4.0;

// A plate 60 x 50 x 5 with its top at z = 0, and on it a block 20 x 10 x
// 15 from the origin: its front at y = 0, its ends at x = 0 and x = 20.
ShapePtr block_on_plate() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, -5), gp_Pnt(40, 30, 0)).Shape();
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 10, 15)).Shape();
  BRepAlgoAPI_Fuse fused(plate, block);
  fused.SimplifyResult();
  CHECK(fused.IsDone());
  return import_body(fused.Shape(), "F1", 0);
}

// The face of the shape through a point inside it; -1 for none.
int face_at(const Shape& shape, const gp_Pnt& point) {
  const TopoDS_Vertex at = BRepBuilderAPI_MakeVertex(point).Vertex();
  for (int f = 0; f < shape.face_count(); ++f) {
    BRepExtrema_DistShapeShape distance(at, shape.face(f));
    if (distance.IsDone() && distance.Value() < 1.0e-7) {
      return f;
    }
  }
  return -1;
}

std::string face_name_at(const Shape& shape, const gp_Pnt& point) {
  const int f = face_at(shape, point);
  CHECK(f >= 0);
  return f >= 0 && !shape.face_names(f).empty() ? shape.face_names(f).front() : std::string();
}

// The name of the one edge between two faces.
std::string edge_between(const Shape& shape, const std::string& a, const std::string& b) {
  const std::vector<int> found = shape.find_edges(make_edge_name(a, b));
  CHECK(found.size() == 1);
  return found.size() == 1 ? shape.edge_name(found.front()) : std::string();
}

struct Foot {
  ShapePtr body;
  std::string plate; // the plate's top
  std::string front;
  std::string left;
  std::string right;
  std::string front_edge;
  std::string left_edge;
  std::string right_edge;
};

Foot foot() {
  Foot made;
  made.body = block_on_plate();
  const Shape& body = *made.body;
  made.plate = face_name_at(body, gp_Pnt(-15, -15, 0));
  made.front = face_name_at(body, gp_Pnt(10, 0, 7));
  made.left = face_name_at(body, gp_Pnt(0, 5, 7));
  made.right = face_name_at(body, gp_Pnt(20, 5, 7));
  made.front_edge = edge_between(body, made.front, made.plate);
  made.left_edge = edge_between(body, made.left, made.plate);
  made.right_edge = edge_between(body, made.right, made.plate);
  return made;
}

bool mentions(const std::vector<std::string>& notes, const char* text) {
  return std::any_of(notes.begin(), notes.end(),
                     [text](const std::string& note) { return note.find(text) != std::string::npos; });
}

// The three foot edges rounded with 4 mm: a valid solid whose ends'
// roundings run past the front's plane to their mitres with the front's,
// and no warning of a rounding hanging out.
void test_three_foot_edges() {
  const Foot body = foot();
  CHECK(!body.front_edge.empty() && !body.left_edge.empty() && !body.right_edge.empty());
  const ShapePtr rounded =
      fillet("F2", *body.body, std::vector<std::string>{body.left_edge, body.front_edge, body.right_edge}, kRadius);
  CHECK(rounded != nullptr);
  if (!rounded) {
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
  // Each end's rounding reaches past the front's plane (y = 0) to its
  // mitre, as far as the rounding's contact on the plate (y = -4 there).
  for (const std::string& end : {body.left_edge, body.right_edge}) {
    const std::vector<int> faces = rounded->find_faces("F2:fillet(" + end + ")");
    CHECK(!faces.empty());
    Bnd_Box box;
    for (const int f : faces) {
      BRepBndLib::AddOptimal(rounded->face(f), box, false, false);
    }
    double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
    if (!box.IsVoid()) {
      box.Get(x0, y0, z0, x1, y1, z1);
    }
    std::printf("  rounding of %s: y from %.4f to %.4f\n", end.c_str(), y0, y1);
    CHECK(!box.IsVoid() && y0 < -1.0);
  }
  const std::string warning =
      detail::dressup_warning(*body.body, *rounded, "F2", "fillet", detail::DressupSide::Outside, kRadius);
  std::printf("  warning: %s\n", warning.empty() ? "none" : warning.c_str());
  CHECK(warning.find("hangs") == std::string::npos);
  CHECK(!mentions(rounded->notes(), "hangs"));
}

// The ends' foot edges rounded alone: every edge of the result between the
// front and the plate comes from the front's foot edge before; the
// roundings' faces and edges are the fillet's own; the faces it changed
// keep their names.
void test_preview_items_before() {
  const Foot body = foot();
  const ShapePtr rounded = fillet("F2", *body.body, std::vector<std::string>{body.left_edge, body.right_edge}, kRadius);
  CHECK(rounded != nullptr);
  if (!rounded) {
    return;
  }
  const std::vector<int> pieces = rounded->find_edges(make_edge_name(body.front, body.plate));
  CHECK(!pieces.empty());
  bool middle = false;
  for (const int e : pieces) {
    const std::string& name = rounded->edge_name(e);
    const BRepAdaptor_Curve curve(rounded->edge(e));
    const gp_Pnt mid = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    middle = middle || (mid.X() > 1.0 && mid.X() < 19.0);
    const ItemOrigin origin = item_origin(*body.body, rounded.get(), 'E', name);
    std::printf("  %s (middle at x %.3f): %s\n", name.c_str(), mid.X(), origin.name.c_str());
    CHECK(origin.kind == ItemOrigin::Kind::Same || origin.kind == ItemOrigin::Kind::Modified);
    CHECK(origin.name == body.front_edge);
    // By the names alone (no result to look at): the one edge between them.
    CHECK(item_origin(*body.body, nullptr, 'E', name).name == body.front_edge);
  }
  // The piece between the roundings, where a click on the foot edge falls.
  CHECK(middle);
  // matching_edges: the edges find_edges finds, without its order
  // (mitcad#1464).
  std::vector<int> ordered = pieces;
  std::sort(ordered.begin(), ordered.end());
  CHECK(rounded->matching_edges(make_edge_name(body.front, body.plate)) == ordered);
  // A face, edge or vertex a result shares with the body before (here the
  // body before itself as the result) is itself, found without measuring
  // (mitcad#1464).
  for (int f = 0; f < body.body->face_count(); ++f) {
    if (!body.body->face_names(f).empty()) {
      const std::string& name = body.body->face_names(f).front();
      const ItemOrigin same = item_origin(*body.body, body.body.get(), 'F', name);
      CHECK(same.kind == ItemOrigin::Kind::Same && same.name == name);
    }
  }
  for (int e = 0; e < body.body->edge_count(); ++e) {
    const std::string& name = body.body->edge_name(e);
    if (!name.empty()) {
      const ItemOrigin same = item_origin(*body.body, body.body.get(), 'E', name);
      CHECK(same.kind == ItemOrigin::Kind::Same && same.name == name);
    }
  }
  // The faces the fillet changed keep their names.
  const ItemOrigin front = item_origin(*body.body, rounded.get(), 'F', body.front);
  CHECK(front.kind == ItemOrigin::Kind::Same && front.name == body.front);
  // The roundings and their edges are the fillet's own.
  const std::string rounding = "F2:fillet(" + body.left_edge + ")";
  CHECK(!rounded->find_faces(rounding).empty());
  const std::vector<int> own_faces = rounded->find_faces(rounding);
  if (!own_faces.empty()) {
    const ItemOrigin made = item_origin(*body.body, rounded.get(), 'F', rounded->face_names(own_faces.front()).front());
    CHECK(made.kind == ItemOrigin::Kind::Made && made.name.empty());
  }
  int own_edges = 0;
  for (int e = 0; e < rounded->edge_count(); ++e) {
    const std::string& name = rounded->edge_name(e);
    if (name.find("F2:fillet(") == std::string::npos) {
      continue;
    }
    ++own_edges;
    CHECK(item_origin(*body.body, rounded.get(), 'E', name).kind == ItemOrigin::Kind::Made);
  }
  CHECK(own_edges > 0);
  // An edge between faces that never met before has no origin to tell.
  const ItemOrigin lost = item_origin(*body.body, rounded.get(), 'E', make_edge_name(body.left, body.right));
  CHECK(lost.kind == ItemOrigin::Kind::Lost);
}

} // namespace
} // namespace test

int main() {
  test::guarded("three foot edges", test::test_three_foot_edges);
  test::guarded("preview items before", test::test_preview_items_before);
  if (test::failures == 0) {
    std::puts("test_fillet_foot_mitre: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
