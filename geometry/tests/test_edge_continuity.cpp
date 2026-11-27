// SPDX-License-Identifier: MIT
// Continuity Analysis (mitcad#2521): the gap, the angle between the normals
// and the difference of the normal curvatures across a body's edges.
//
// A 40 x 30 x 20 box: each of its twelve edges is G0 only, its faces
// square to each other (90 degrees) with no gap. Its top front edge
// rounded 4 mm (tangent): the rounding's two long edges are G1, the
// normals agreeing (0 degrees) and the curvature across jumping from 0 to
// 1/4 (100 %), so not G2; its ends on the side faces stay G0. Rounded 4 mm
// curvature continuous (G2): the long edges are G2 within the default
// tolerances. A cylinder's seam is G2, its circles G0. A lone face's edges
// are free: not measured. The tolerances decide the class: a G1 edge is G2
// for a tolerance above its curvature share, G0 for an angle tolerance of 0.

#include <array>
#include <cmath>
#include <cstdio>
#include <memory>
#include <string>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "bundled_test.hpp"
#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/edge_continuity.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kWidth = 40.0;
constexpr double kDepth = 30.0;
constexpr double kHeight = 20.0;
constexpr double kRadius = 4.0;
constexpr int kSamples = 16;

// The box from the origin, its faces named B:x0, B:x1, B:y0, B:y1, B:z0,
// B:z1 by the side they lie on.
ShapePtr box() {
  const TopoDS_Shape solid = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), kWidth, kDepth, kHeight).Shape();
  std::vector<Shape::NamedFace> named;
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box bounds;
    BRepBndLib::Add(f.Current(), bounds, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    bounds.Get(x1, y1, z1, x2, y2, z2);
    const auto flat = [](double a, double b) { return std::abs(a - b) < 1e-3; };
    std::string name = "B:";
    if (flat(x1, x2)) {
      name += x1 < 1.0 ? "x0" : "x1";
    } else if (flat(y1, y2)) {
      name += y1 < 1.0 ? "y0" : "y1";
    } else {
      name += z1 < 1.0 ? "z0" : "z1";
    }
    named.push_back({f.Current(), {name}});
  }
  return std::make_shared<Shape>(solid, named);
}

// The name of the body's edge from a to b (either way round); empty for none.
std::string edge_named(const Shape& body, const gp_Pnt& a, const gp_Pnt& b) {
  for (int e = 0; e < body.edge_count(); ++e) {
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(body.edge(e), first, last);
    const gp_Pnt p = BRep_Tool::Pnt(first);
    const gp_Pnt q = BRep_Tool::Pnt(last);
    if ((p.Distance(a) < 1e-6 && q.Distance(b) < 1e-6) || (p.Distance(b) < 1e-6 && q.Distance(a) < 1e-6)) {
      return body.edge_name(e);
    }
  }
  return std::string();
}

// The box's top front edge, along x at y 0, z 20.
std::string top_front(const Shape& body) {
  return edge_named(body, gp_Pnt(0, 0, kHeight), gp_Pnt(kWidth, 0, kHeight));
}

bool is_rounding(const Shape& shape, int face) {
  if (face < 0) {
    return false;
  }
  for (const std::string& name : shape.face_names(face)) {
    if (name.rfind("F5:fillet(", 0) == 0) {
      return true;
    }
  }
  return false;
}

// The rounding's edges along it (between it and a face of the box) and
// across its ends (likewise, on the side faces): those where one face is
// the rounding and the other is not, told apart by their length.
struct RoundingEdges {
  std::vector<int> along;
  std::vector<int> ends;
};

RoundingEdges rounding_edges(const Shape& shape) {
  RoundingEdges found;
  for (int e = 0; e < shape.edge_count(); ++e) {
    const std::array<int, 2> faces = shape.edge_faces(e);
    if (is_rounding(shape, faces[0]) == is_rounding(shape, faces[1])) {
      continue;
    }
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(shape.edge(e), first, last);
    const double chord = BRep_Tool::Pnt(first).Distance(BRep_Tool::Pnt(last));
    (chord > kWidth / 2 ? found.along : found.ends).push_back(e);
  }
  return found;
}

const EdgeContinuity* measured_edge(const std::vector<EdgeContinuity>& all, int edge) {
  for (const EdgeContinuity& item : all) {
    if (item.edge == edge) {
      return &item;
    }
  }
  return nullptr;
}

void test_box_edges_are_g0() {
  const ShapePtr body = box();
  const std::vector<EdgeContinuity> all = edge_continuity(*body, {}, kSamples);
  CHECK(all.size() == 12);
  for (const EdgeContinuity& edge : all) {
    CHECK(edge.sides == EdgeSides::Shared);
    CHECK(edge.reached == ContinuityClass::G0);
    CHECK(edge.samples == kSamples + 1);
    CHECK(edge.points.size() == static_cast<std::size_t>(kSamples + 1));
    CHECK(edge.gap < 1e-9);
    CHECK(std::abs(edge.angle - kPi / 2) < 1e-9);
    CHECK(!edge.name.empty());
  }
  // Some edges only, by index; an index twice or out of range counts once
  // or not at all.
  const std::vector<EdgeContinuity> two = edge_continuity(*body, {0, 3, 3, 99}, kSamples);
  CHECK(two.size() == 2);
  // The edges of the top face: its four.
  int top = -1;
  for (int f = 0; f < body->face_count(); ++f) {
    if (body->face_names(f).front() == "B:z1") {
      top = f;
    }
  }
  CHECK(edges_of_faces(*body, {top}).size() == 4);
  CHECK(edges_of_faces(*body, {top, top}).size() == 4);
}

void test_tangent_rounding_is_g1() {
  const ShapePtr body = box();
  const std::string edge = top_front(*body);
  CHECK(!edge.empty());
  const ShapePtr rounded = fillet("F5", *body, std::vector<std::string>{edge}, kRadius);
  const RoundingEdges edges = rounding_edges(*rounded);
  CHECK(edges.along.size() == 2);
  CHECK(edges.ends.size() == 2);
  const std::vector<EdgeContinuity> all = edge_continuity(*rounded, {}, kSamples);
  for (const int e : edges.along) {
    const EdgeContinuity* found = measured_edge(all, e);
    CHECK(found != nullptr);
    if (found == nullptr) {
      continue;
    }
    if (found->reached != ContinuityClass::G1) {
      std::fprintf(stderr, "tangent rounding: gap %g, angle %g rad, curvature %g (%g)\n", found->gap, found->angle,
                   found->curvature, found->curvature_share);
    }
    CHECK(found->reached == ContinuityClass::G1);
    CHECK(found->gap < 1e-6);
    CHECK(found->angle < 1e-6);
    CHECK_NEAR_TOL(found->curvature, 1.0 / kRadius, 1e-4);
    CHECK_NEAR_TOL(found->curvature_share, 1.0, 1e-4);
  }
  for (const int e : edges.ends) {
    const EdgeContinuity* found = measured_edge(all, e);
    CHECK(found != nullptr && found->reached == ContinuityClass::G0);
    CHECK(found != nullptr && std::abs(found->angle - kPi / 2) < 1e-6);
  }
  // The rounding's own edges: its two long ones and its two ends.
  int rounding = -1;
  for (int f = 0; f < rounded->face_count(); ++f) {
    rounding = is_rounding(*rounded, f) ? f : rounding;
  }
  CHECK(edges_of_faces(*rounded, {rounding}).size() == 4);
  // The tolerances decide: a curvature tolerance above the share makes the
  // long edges G2, an angle tolerance of 0 keeps them G0 only where the
  // normals differ at all.
  if (!edges.along.empty()) {
    const EdgeContinuity* found = measured_edge(all, edges.along.front());
    if (found != nullptr) {
      ContinuityTolerances loose;
      loose.curvature = 1.5;
      CHECK(continuity_class(*found, loose) == ContinuityClass::G2);
      ContinuityTolerances tight;
      tight.gap = -1.0;
      CHECK(continuity_class(*found, tight) == ContinuityClass::None);
    }
  }
}

void test_curvature_continuous_rounding_is_g2() {
  const ShapePtr body = box();
  FilletSet set;
  set.edges = {top_front(*body)};
  set.radius = kRadius;
  set.curvature = true;
  const ShapePtr rounded = fillet("F5", *body, std::vector<FilletSet>{set});
  const RoundingEdges edges = rounding_edges(*rounded);
  CHECK(edges.along.size() == 2);
  const std::vector<EdgeContinuity> all = edge_continuity(*rounded, edges.along, kSamples);
  CHECK(all.size() == edges.along.size());
  const ContinuityTolerances tolerances;
  for (const EdgeContinuity& found : all) {
    if (found.reached != ContinuityClass::G2) {
      std::fprintf(stderr, "G2 rounding: gap %g, angle %g rad, curvature %g (%g)\n", found.gap, found.angle,
                   found.curvature, found.curvature_share);
    }
    CHECK(found.reached == ContinuityClass::G2);
    CHECK(found.gap <= tolerances.gap);
    CHECK(found.angle <= tolerances.angle);
    CHECK(found.curvature_share <= tolerances.curvature);
    CHECK(found.samples > 0);
  }
}

void test_seam_and_free_edges() {
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(5.0, 10.0).Shape();
  std::vector<Shape::NamedFace> named;
  int index = 0;
  for (TopExp_Explorer f(cylinder, TopAbs_FACE); f.More(); f.Next()) {
    named.push_back({f.Current(), {"C:f" + std::to_string(index++)}});
  }
  const Shape body(cylinder, named);
  int seams = 0;
  int circles = 0;
  for (const EdgeContinuity& edge : edge_continuity(body, {}, kSamples)) {
    if (edge.sides == EdgeSides::Seam) {
      ++seams;
      CHECK(edge.reached == ContinuityClass::G2);
      CHECK(edge.gap < 1e-9);
      CHECK(edge.angle < 1e-9);
      CHECK(edge.curvature < 1e-6);
    } else {
      ++circles;
      CHECK(edge.sides == EdgeSides::Shared);
      CHECK(edge.reached == ContinuityClass::G0);
      CHECK(std::abs(edge.angle - kPi / 2) < 1e-6);
    }
  }
  CHECK(seams == 1);
  CHECK(circles == 2);

  const TopoDS_Shape face = BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 0, 10, 0, 10).Face();
  const Shape lone(face, {{face, {"P:f"}}});
  const std::vector<EdgeContinuity> free_edges = edge_continuity(lone, {}, kSamples);
  CHECK(free_edges.size() == 4);
  for (const EdgeContinuity& edge : free_edges) {
    CHECK(edge.sides == EdgeSides::Open);
    CHECK(edge.reached == ContinuityClass::Unmeasured);
    CHECK(edge.samples == 0);
    CHECK(edge.points.size() == static_cast<std::size_t>(kSamples + 1));
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(edge_continuity) {
  test::guarded("test_box_edges_are_g0", test::test_box_edges_are_g0);
  test::guarded("test_tangent_rounding_is_g1", test::test_tangent_rounding_is_g1);
  test::guarded("test_curvature_continuous_rounding_is_g2", test::test_curvature_continuous_rounding_is_g2);
  test::guarded("test_seam_and_free_edges", test::test_seam_and_free_edges);
  if (test::failures == 0) {
    std::puts("test_edge_continuity: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
