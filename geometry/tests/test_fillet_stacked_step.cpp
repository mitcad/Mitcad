// SPDX-License-Identifier: MIT
// Two stacked profiles with a step (GitHub mitcad/Mitcad#536, mitcad#2165):
// a profile 10 x 5 extruded 10 high, the profile 10 x 5 beside it (sharing
// its long edge) extruded 5 high and joined. Its four vertical edges that
// bound one vertical face each, the upper face's two (on top of the step)
// and the lower face's two (at the far end), rounded with 5: each pair takes
// its 10 wide face away (two roundings of 5 that meet in its middle), and
// the upper pair takes the upper part of both end faces away too (5 wide).
// The step's ledge between them narrows to a point at each end of the upper
// face's rounding.
//
// As one feature the fillet must build at its exact size with no warning,
// and equal the same edges rounded as two features of two edges each: the
// two pairs do not reach each other (the upper pair's roundings end on the
// ledge, the lower pair's below it), so the body loses 5 x (100 - 25 pi),
// half of it at each pair: 250 + 125 pi is left. Where the geometry
// kernel's build of all four at once fails, the pairs, groups whose
// roundings cannot meet, are built one after the other at their exact
// size, kept because the volumes they take away add up (groups_apart in
// dressup.cpp). The kernel's own status for each combination of the edges
// is printed first, for the log.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kRadius = 5.0;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int count_of(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer ex(shape, type); ex.More(); ex.Next()) {
    ++n;
  }
  return n;
}

std::string notes(const Shape& shape) {
  std::string text;
  for (const std::string& note : shape.notes()) {
    text += (text.empty() ? "" : "; ") + note;
  }
  return text;
}

// A rectangle from (x, y), its sides named as the sketch names them:
// `names` bottom, right, top and left.
Region named_rectangle(double x, double y, double width, double height, const std::vector<std::string>& names,
                       const std::string& region) {
  Region r = rectangle_region(x, y, width, height);
  r.name = region;
  for (std::size_t i = 0; i < 4; ++i) {
    r.loops[0].segments[i].name = names[i];
  }
  return r;
}

ShapePtr extruded(const std::string& feature, Region region, double end) {
  ExtrudeSpec spec;
  spec.feature = feature;
  spec.regions = {std::move(region)};
  spec.start = 0.0;
  spec.end = end;
  return extrude(spec);
}

// The design's body: Extrude1 the lower profile (y 0 to 5) 10 high, Extrude2
// the upper one (y 5 to 10) 5 high, joined.
ShapePtr stacked_body() {
  const ShapePtr tall = extruded("F2",
                                 named_rectangle(0, 0, 10, 5,
                                                 {"c10[c13,c11]", "c11[c10,c12+c15]", "c12[c11+c15,c13+c17]",
                                                  "c13[c12+c17,c10]"},
                                                 "r{c10[c13,c11],c11[c10,c12+c15],c12[c11+c15,c13+c17],c13[c12+c17,c10]}"),
                                 10.0);
  const ShapePtr low = extruded("F3",
                                named_rectangle(0, 5, 10, 5,
                                                {"c12[c11+c15,c13+c17]", "c15[c11+c12,c16]", "c16[c15,c17]",
                                                 "c17[c16,c12+c13]"},
                                                "r{c12[c11+c15,c13+c17],c15[c11+c12,c16],c16[c15,c17],c17[c16,c12+c13]}"),
                                5.0);
  return boolean(BooleanOp::Join, {tall.get()}, *low).pieces.at(0).shape;
}

// The vertical edge at (x, y) whose middle is at height z, or null.
TopoDS_Edge vertical_edge(const TopoDS_Shape& shape, double x, double y, double z) {
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge e = TopoDS::Edge(ex.Current());
    BRepAdaptor_Curve c(e);
    if (c.GetType() != GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
    const gp_Pnt m((a.XYZ() + b.XYZ()) / 2.0);
    if (std::abs(a.X() - x) < 1e-7 && std::abs(b.X() - x) < 1e-7 && std::abs(a.Y() - y) < 1e-7 &&
        std::abs(b.Y() - y) < 1e-7 && std::abs(m.Z() - z) < 1e-7) {
      return e;
    }
  }
  return TopoDS_Edge();
}

struct Spot {
  double x, y, z;
};

// The upper face's two edges (on top of the step), then the lower face's
// two (at the far end), as the reported feature lists them.
const std::vector<Spot> kUpper = {{10, 5, 7.5}, {0, 5, 7.5}};
const std::vector<Spot> kLower = {{10, 10, 2.5}, {0, 10, 2.5}};

std::string edge_named_at(const Shape& body, const Spot& spot) {
  const TopoDS_Edge e = vertical_edge(body.occt(), spot.x, spot.y, spot.z);
  if (e.IsNull()) {
    return {};
  }
  const int index = body.edge_index(e);
  return index < 0 ? std::string() : body.edge_name(index);
}

FilletSet set_at(const Shape& body, const std::vector<Spot>& spots) {
  FilletSet set;
  set.radius = kRadius;
  for (const Spot& spot : spots) {
    const std::string name = edge_named_at(body, spot);
    CHECK(!name.empty());
    set.edges.push_back(name);
  }
  return set;
}

// What the geometry kernel's own fillet does with the edges at `spots` of a
// copy of `body`, printed: whether it builds, and where not, each contour's
// status and the faulty vertices.
TopoDS_Shape kernel_fillet(const char* label, const TopoDS_Shape& body, const std::vector<Spot>& spots) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(body).Shape();
  BRepFilletAPI_MakeFillet maker(copy);
  for (const Spot& spot : spots) {
    const TopoDS_Edge e = vertical_edge(copy, spot.x, spot.y, spot.z);
    if (e.IsNull()) {
      std::printf("  %s: no edge at (%g, %g, %g)\n", label, spot.x, spot.y, spot.z);
      return TopoDS_Shape();
    }
    maker.Add(kRadius, e);
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("  %s: threw %s\n", label, failure.what());
    return TopoDS_Shape();
  }
  if (maker.IsDone()) {
    const TopoDS_Shape made = maker.Shape();
    std::printf("  %s: built, volume %.9f, %d faces, valid %d\n", label, volume_of(made),
                count_of(made, TopAbs_FACE), BRepCheck_Analyzer(made).IsValid() ? 1 : 0);
    return made;
  }
  std::printf("  %s: failed; %d contours, %d faulty, %d faulty vertices, partial result %d\n", label,
              maker.NbContours(), maker.NbFaultyContours(), maker.NbFaultyVertices(), maker.HasResult() ? 1 : 0);
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    std::printf("    contour %d: status %d, %d surfaces\n", ic, static_cast<int>(maker.StripeStatus(ic)),
                maker.NbComputedSurfaces(ic));
  }
  for (int iv = 1; iv <= maker.NbFaultyVertices(); ++iv) {
    const gp_Pnt p = BRep_Tool::Pnt(maker.FaultyVertex(iv));
    std::printf("    faulty vertex %d at (%.6f, %.6f, %.6f)\n", iv, p.X(), p.Y(), p.Z());
  }
  return TopoDS_Shape();
}

std::vector<Spot> joined(std::vector<Spot> a, const std::vector<Spot>& b) {
  a.insert(a.end(), b.begin(), b.end());
  return a;
}

// The kernel's fillet of each combination of the edges, and of the pairs
// one after the other: where the one-feature build first departs from the
// two-feature one, printed for the run's log (no checks: MitCad builds a
// pair the kernel cannot by its path past narrow faces).
void test_kernel_alone() {
  const ShapePtr body = stacked_body();
  std::printf("body: volume %.9f, %d faces\n", volume_of(body->occt()), count_of(body->occt(), TopAbs_FACE));
  CHECK_NEAR_TOL(volume_of(body->occt()), 750.0, 1e-9);
  kernel_fillet("all four", body->occt(), joined(kUpper, kLower));
  kernel_fillet("all four, lower first", body->occt(), joined(kLower, kUpper));
  const TopoDS_Shape upper = kernel_fillet("upper pair", body->occt(), kUpper);
  const TopoDS_Shape lower = kernel_fillet("lower pair", body->occt(), kLower);
  for (const Spot& one : kLower) {
    kernel_fillet("upper pair and one lower", body->occt(), joined(kUpper, {one}));
  }
  for (const Spot& one : kUpper) {
    kernel_fillet("lower pair and one upper", body->occt(), joined(kLower, {one}));
  }
  for (const Spot& u : kUpper) {
    for (const Spot& l : kLower) {
      kernel_fillet("one upper, one lower", body->occt(), {u, l});
    }
  }
  if (!upper.IsNull()) {
    kernel_fillet("lower pair after the upper", upper, kLower);
  }
  if (!lower.IsNull()) {
    kernel_fillet("upper pair after the lower", lower, kUpper);
  }
}

// Every vertex of `a` within `tolerance` of `b`.
bool vertices_on(const TopoDS_Shape& a, const TopoDS_Shape& b, double tolerance) {
  bool all = true;
  for (TopExp_Explorer ex(a, TopAbs_VERTEX); ex.More(); ex.Next()) {
    BRepExtrema_DistShapeShape distance(ex.Current(), b);
    if (!distance.IsDone() || distance.Value() > tolerance) {
      const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
      std::printf("  vertex (%.6f, %.6f, %.6f) off by %.3g\n", p.X(), p.Y(), p.Z(),
                  distance.IsDone() ? distance.Value() : -1.0);
      all = false;
    }
  }
  return all;
}

// The reported case: the four edges in one feature build at their exact
// size, with no warning, as the two features do.
void test_one_feature_equals_two() {
  const ShapePtr body = stacked_body();
  const double expected = 250.0 + 125.0 * kPi;
  // Two features: the upper pair, then the lower pair on its result.
  ShapePtr first;
  ShapePtr two;
  try {
    first = fillet("F4", *body, {set_at(*body, kUpper)});
    two = fillet("F5", *first, {set_at(*first, kLower)});
  } catch (const std::exception& failure) {
    std::printf("  two features: %s\n", failure.what());
  }
  CHECK(two != nullptr);
  if (two) {
    std::printf("two features: volume %.9f, %d faces, notes: %s\n", volume_of(two->occt()),
                count_of(two->occt(), TopAbs_FACE), (notes(*first) + " / " + notes(*two)).c_str());
    CHECK(first->notes().empty());
    CHECK(two->notes().empty());
    CHECK(BRepCheck_Analyzer(two->occt()).IsValid());
    CHECK_NEAR_TOL(volume_of(two->occt()), expected, 1e-9);
  }
  // One feature of the four edges, as the reported feature lists them.
  ShapePtr one;
  try {
    one = fillet("F4", *body, {set_at(*body, joined(kUpper, kLower))});
  } catch (const std::exception& failure) {
    std::printf("  one feature: %s\n", failure.what());
  }
  CHECK(one != nullptr);
  if (!one) {
    return;
  }
  std::printf("one feature: volume %.9f, %d faces, notes: %s\n", volume_of(one->occt()),
              count_of(one->occt(), TopAbs_FACE), notes(*one).c_str());
  CHECK(one->notes().empty());
  CHECK(BRepCheck_Analyzer(one->occt()).IsValid());
  CHECK(count_of(one->occt(), TopAbs_SOLID) == 1);
  CHECK_NEAR_TOL(volume_of(one->occt()), expected, 1e-9);
  if (two) {
    CHECK(count_of(one->occt(), TopAbs_FACE) == count_of(two->occt(), TopAbs_FACE));
    // (The baseline's tolerance, 0.000001 mm.)
    CHECK(vertices_on(one->occt(), two->occt(), 1e-6));
    CHECK(vertices_on(two->occt(), one->occt(), 1e-6));
  }
  // Each rounding named after its edge.
  int rounded = 0;
  for (int f = 0; f < one->face_count(); ++f) {
    for (const std::string& name : one->face_names(f)) {
      rounded += name.rfind("F4:fillet(", 0) == 0 ? 1 : 0;
    }
  }
  CHECK(rounded >= 4);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_kernel_alone", test::test_kernel_alone);
  test::guarded("test_one_feature_equals_two", test::test_one_feature_equals_two);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("all tests passed\n");
  return 0;
}
