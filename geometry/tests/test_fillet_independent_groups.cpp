// SPDX-License-Identifier: MIT
// A fillet whose edges lie in groups the geometry kernel rounds apart from
// each other (no face or vertex shared: the copies of a pattern's rounding
// at its elements), one group needing a way round the kernel's failure
// (mitcad#2161). Where the edges do not build at their exact size together,
// each group is built on its own, one after another, with the ways round a
// failure; before, those ways ran on all the groups at once (on a
// pattern's seven copies, a narrow face continued at one element, 90 s
// where the copies one by one took 5 s), and a smaller retry took all of
// them 0.1 % smaller.
//
// The body: a plate with three ribs along y on its top, 20 long and 4
// high: two 3 wide and one 2 wide between them. Both top edges of every
// rib rounded with 1 in one fillet: the 2 wide rib's top is taken away
// exactly (a full round, which the kernel does not build at its exact
// size), the others' tops stay 1 wide. The check: one valid closed solid;
// the material each 3 wide rib loses exactly two rounding sections along
// its length; the whole fillet's loss that of the three ribs rounded each
// on its own (within 1e-6).

#include <cmath>
#include <cstdio>
#include <memory>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <Geom_Curve.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/dressup.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kLength = 20.0; // the ribs' length (y)
constexpr double kTop = 9.0;     // the ribs' top (the plate 5 thick, the ribs 4 high)
constexpr double kRadius = 1.0;

struct Rib {
  double x0;
  double x1;
};
const std::vector<Rib> kRibs = {{-40.0, -37.0}, {-1.0, 1.0}, {37.0, 40.0}};

// The body with its faces named N1, N2, ... in TopExp order.
ShapePtr named(const TopoDS_Shape& body) {
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"N" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(body, faces);
}

double volume_of(const TopoDS_Shape& shape) { return mitcad::analysis::physical_properties(shape).volume; }

bool closed_valid_solid(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1 && BRepCheck_Analyzer(shape).IsValid();
}

// The plate with the ribs.
ShapePtr ribbed() {
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(-60, 0, 0), gp_Pnt(60, kLength, 5)).Shape();
  for (const Rib& rib : kRibs) {
    body = BRepAlgoAPI_Fuse(body, BRepPrimAPI_MakeBox(gp_Pnt(rib.x0, 0, 5), gp_Pnt(rib.x1, kLength, kTop)).Shape())
               .Shape();
  }
  return named(body);
}

// The names of the top edges of the ribs `which` (indices into kRibs).
std::vector<std::string> top_edges(const Shape& body, const std::vector<int>& which) {
  std::vector<std::string> names;
  for (int e = 0; e < body.edge_count(); ++e) {
    double first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(body.edge(e), first, last);
    if (curve.IsNull()) {
      continue;
    }
    const gp_Pnt m = curve->Value(0.5 * (first + last));
    if (std::abs(m.Z() - kTop) > 1e-6 || std::abs(m.Y() - 0.5 * kLength) > 1e-6) {
      continue;
    }
    for (const int r : which) {
      const Rib& rib = kRibs[static_cast<std::size_t>(r)];
      if (std::abs(m.X() - rib.x0) < 1e-6 || std::abs(m.X() - rib.x1) < 1e-6) {
        names.push_back(body.edge_name(e));
      }
    }
  }
  return names;
}

// The ribs `which` rounded in one fillet; null when it fails.
ShapePtr rounded(const Shape& body, const std::vector<int>& which) {
  FilletSet set;
  set.edges = top_edges(body, which);
  set.radius = kRadius;
  CHECK(set.edges.size() == 2 * which.size());
  try {
    return fillet("R", body, {set}, true);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "fillet of ribs %zu: %s\n", which.size(), error.what());
    return nullptr;
  }
}

void test_ribs_rounded_together_as_each_alone() {
  const ShapePtr body = ribbed();
  const double before = volume_of(body->occt());
  const ShapePtr all = rounded(*body, {0, 1, 2});
  CHECK(all != nullptr);
  if (!all) {
    return;
  }
  CHECK(closed_valid_solid(all->occt()));
  double alone = 0.0;
  for (int r = 0; r < 3; ++r) {
    const ShapePtr one = rounded(*body, {r});
    CHECK(one != nullptr);
    if (!one) {
      return;
    }
    const double lost = before - volume_of(one->occt());
    // (The 3 wide ribs: two rounding sections, the square less the
    // quarter disc, along the length.)
    if (r != 1) {
      CHECK_NEAR_TOL(lost, 2.0 * fillet_loss(kRadius) * kLength, 1e-6);
    }
    alone += lost;
  }
  CHECK_NEAR_TOL(before - volume_of(all->occt()), alone, 1e-6);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_ribs_rounded_together_as_each_alone", test::test_ribs_rounded_together_as_each_alone);
  if (test::failures == 0) {
    std::printf("test_fillet_independent_groups: all tests passed\n");
  }
  return test::failures == 0 ? 0 : 1;
}
