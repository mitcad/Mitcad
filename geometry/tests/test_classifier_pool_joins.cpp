// SPDX-License-Identifier: MIT
// Joins of B-spline-faced bodies, repeated, with the booleans' parallel mode
// and the solid classifier's pool (the port's patch 1282) on (mitcad#1553).
// A reported join aborted while it freed a B-spline surface's evaluation
// cache in a thread of the pool that makes a classifier's face intersectors
// (BRepClass3d_SolidExplorer::InitShape, from 64 faces on); the reporter's
// design has not arrived. These are the loops that were run under
// ThreadSanitizer and AddressSanitizer in a sanitizer build of the patched
// OCCT, shortened: lofts of 60 B-spline sides, five located instances of one
// loft whose faces share their TShapes (as a pattern's do), and a loft whose
// top rim is rounded, each joined with a loft again and again, the results
// the same each time; and classifiers of a joined solid of 90 faces loaded
// on several threads at once, each with the pool, classifying the same.

#include <atomic>
#include <cmath>
#include <cstdio>
#include <memory>
#include <string>
#include <thread>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/query.hpp"

namespace test {
namespace {

using mitcad::geometry::BooleanOp;
using mitcad::geometry::BooleanResult;
using mitcad::geometry::Shape;

// A wire of n arcs bulging outwards round a circle of radius r at height z,
// turned by phase: a loft through such wires has n B-spline side faces.
TopoDS_Wire wavy(int n, double r, double z, double phase) {
  BRepBuilderAPI_MakeWire wire;
  for (int i = 0; i < n; ++i) {
    const double a0 = phase + 2.0 * kPi * i / n;
    const double a1 = phase + 2.0 * kPi * (i + 1) / n;
    const double am = 0.5 * (a0 + a1);
    const double rm = r * 1.04;
    const gp_Pnt p0(r * std::cos(a0), r * std::sin(a0), z);
    const gp_Pnt pm(rm * std::cos(am), rm * std::sin(am), z);
    const gp_Pnt p1(r * std::cos(a1), r * std::sin(a1), z);
    wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p0, pm, p1).Value()).Edge());
  }
  return wire.Wire();
}

// A smooth loft of n B-spline sides through three such wires, 16 high,
// moved by (shift, 0.3 shift, 0.5).
TopoDS_Shape loft(int n, double shift, double turn) {
  BRepOffsetAPI_ThruSections sections(true, false);
  sections.AddWire(wavy(n, 10.0, 0.0, turn));
  sections.AddWire(wavy(n, 7.0, 8.0, turn + 0.2));
  sections.AddWire(wavy(n, 9.0, 16.0, turn + 0.1));
  sections.Build();
  gp_Trsf move;
  move.SetTranslation(gp_Vec(shift, 0.3 * shift, 0.5));
  return BRepBuilderAPI_Transform(sections.Shape(), move, true).Shape();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum kind) {
  int n = 0;
  for (TopExp_Explorer it(shape, kind); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The join of the tool with the target through MitCad's boolean, `times`
// times: one valid solid of the same faces and volume each time.
void join_repeatedly(const TopoDS_Shape& target, const TopoDS_Shape& tool, int times, int min_faces) {
  const Shape a(target);
  const Shape b(tool);
  int faces = -1;
  double volume = 0.0;
  for (int i = 0; i < times; ++i) {
    const BooleanResult result = mitcad::geometry::boolean(BooleanOp::Join, {&a}, b);
    CHECK(result.pieces.size() == 1);
    if (result.pieces.size() != 1) {
      return;
    }
    const TopoDS_Shape& joined = result.pieces.front().shape->occt();
    CHECK(count(joined, TopAbs_SOLID) == 1);
    CHECK(BRepCheck_Analyzer(joined).IsValid());
    if (i == 0) {
      faces = count(joined, TopAbs_FACE);
      volume = mitcad::geometry::volume(*result.pieces.front().shape);
      // The classifier of the joined solid makes its faces' intersectors on
      // the pool (64 faces or more).
      CHECK(faces >= min_faces);
      CHECK(volume > volume_of(target));
    } else {
      CHECK(count(joined, TopAbs_FACE) == faces);
      CHECK_NEAR_TOL(mitcad::geometry::volume(*result.pieces.front().shape), volume, 1e-9);
    }
  }
}

void test_joins_of_lofts() {
  join_repeatedly(loft(60, 0.0, 0.0), loft(61, 6.0, 0.05), 10, 64);
}

void test_joins_of_a_rounded_loft() {
  const TopoDS_Shape body = loft(50, 0.0, 0.0);
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer it(body, TopAbs_EDGE); it.More(); it.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(it.Current()));
    const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(middle.Z() - 16.5) < 1e-3) {
      fillet.Add(0.3, TopoDS::Edge(it.Current()));
    }
  }
  fillet.Build();
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(count(fillet.Shape(), TopAbs_FACE) > 100);
  join_repeatedly(fillet.Shape(), loft(40, 6.0, 0.0), 6, 64);
}

// Five located instances of one loft, their faces sharing their TShapes,
// joined with a loft lying across them by OCCT's fuse with the settings of
// MitCad's booleans: the solids are split on several threads at once, each
// loading classifiers of faces it shares with the others.
void test_joins_of_pattern_instances() {
  const TopoDS_Shape small = loft(16, 0.0, 0.0);
  NCollection_List<TopoDS_Shape> arguments;
  for (int k = 0; k < 5; ++k) {
    gp_Trsf move;
    move.SetTranslation(gp_Vec(14.0 * k, 0.0, 0.0));
    arguments.Append(small.Moved(TopLoc_Location(move)));
  }
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0.0, 0.0, 8.0), gp_Dir(0.0, 1.0, 0.0)), kPi / 2.0);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(BRepBuilderAPI_Transform(loft(40, 0.0, 0.0), turn, true).Shape());
  int faces = -1;
  double volume = 0.0;
  for (int i = 0; i < 6; ++i) {
    BRepAlgoAPI_Fuse fuse;
    fuse.SetArguments(arguments);
    fuse.SetTools(tools);
    fuse.SetNonDestructive(true);
    fuse.SetRunParallel(true);
    fuse.SetCheckInverted(false);
    fuse.Build();
    CHECK(fuse.IsDone() && !fuse.HasErrors());
    if (!fuse.IsDone() || fuse.HasErrors()) {
      return;
    }
    const TopoDS_Shape& joined = fuse.Shape();
    CHECK(BRepCheck_Analyzer(joined).IsValid());
    if (i == 0) {
      faces = count(joined, TopAbs_FACE);
      volume = volume_of(joined);
      CHECK(faces >= 64);
    } else {
      CHECK(count(joined, TopAbs_FACE) == faces);
      CHECK_NEAR_TOL(volume_of(joined), volume, 1e-9);
    }
  }
}

// Classifiers of one joined solid of 90 faces loaded on four threads at
// once, ten times each: each loads its faces' intersectors on the pool.
void test_classifiers_beside_each_other() {
  const Shape a(loft(60, 0.0, 0.0));
  const Shape b(loft(62, 6.0, 0.0));
  const BooleanResult result = mitcad::geometry::boolean(BooleanOp::Join, {&a}, b);
  CHECK(result.pieces.size() == 1);
  if (result.pieces.size() != 1) {
    return;
  }
  TopExp_Explorer solids(result.pieces.front().shape->occt(), TopAbs_SOLID);
  CHECK(solids.More());
  if (!solids.More()) {
    return;
  }
  const TopoDS_Shape solid = solids.Current();
  CHECK(count(solid, TopAbs_FACE) >= 64);
  std::atomic<int> wrong{0};
  std::vector<std::thread> threads;
  for (int t = 0; t < 4; ++t) {
    threads.emplace_back([&solid, &wrong] {
      for (int i = 0; i < 10; ++i) {
        BRepClass3d_SolidClassifier classifier(solid);
        classifier.Perform(gp_Pnt(3.0, 1.0, 8.0), 1e-7);
        if (classifier.State() != TopAbs_IN) {
          ++wrong;
        }
        classifier.Perform(gp_Pnt(3.0, 1.0, 40.0), 1e-7);
        if (classifier.State() != TopAbs_OUT) {
          ++wrong;
        }
      }
    });
  }
  for (std::thread& thread : threads) {
    thread.join();
  }
  CHECK(wrong.load() == 0);
}

} // namespace
} // namespace test

int main() {
  test::guarded("joins_of_lofts", test::test_joins_of_lofts);
  test::guarded("joins_of_a_rounded_loft", test::test_joins_of_a_rounded_loft);
  test::guarded("joins_of_pattern_instances", test::test_joins_of_pattern_instances);
  test::guarded("classifiers_beside_each_other", test::test_classifiers_beside_each_other);
  if (test::failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("joins of B-spline-faced bodies with the classifier's pool: ok\n");
  return 0;
}
