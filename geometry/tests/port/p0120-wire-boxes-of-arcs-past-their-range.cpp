// SPDX-License-Identifier: MIT
// Port patch 0120: BRepCheck_Face::ClassifyWires skips a pair of wires whose
// UV boxes do not meet. It gave up the box of a wire whose edge's range
// passed its p-curve's own parameters, and then classified that wire
// against every other wire; the arcs of circles of stored bodies do so by a
// rounding error (an arc ending at 2*PI + 4e-15). A circle's or an
// ellipse's box is now that of the conic over the edge's range. The
// statuses are unchanged (geometry.suite.large_bodies,
// test_wires_of_arcs_past_their_range_are_classified); this is the bound.
//
// The bound is a ratio, not a wall-clock limit: a plate of holes of arcs
// past their range is classified about as fast as the same plate with arcs
// ending exactly at 2*PI, whose boxes OCCT kept without the patch too. Both
// plates share every other cost (the wires' intersection check, the pass
// over all pairs that compares their boxes), so the load of the machine
// cancels out of the ratio. Measured on a 32-core Linux workstation with
// 900 holes (810 000 pairs of wires), the shortest of five interleaved runs
// of each plate: with the patch the ratio is 1.0 idle and 0.8 to 2.3 with 40
// copies of the test beside a full check (some 80 runnable threads); without
// it (the unpatched BRepCheck_Face.cxx interposed) it is 24 to 27, 5.1 to
// 5.7 s against 0.21 s, as each pair then classifies a point (some 6 us)
// where the boxes take a fraction of a microsecond. The limit 6 leaves a
// margin of more than 2.5 on the loaded side and 4 on the unpatched side.
// (The earlier bound, 10 000 holes within 4 s, could not hold: the pass
// over the 1e8 pairs alone takes 8 to 12 s with the patch; 565 s without.)

#include <algorithm>
#include <chrono>
#include <cstdio>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Face.hxx>
#include <BRep_Builder.hxx>
#include <Geom_Circle.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>

#include "port_test.hpp"

namespace {

// A plate of n x n cells of 2.5, with a hole of radius 0.5 in each: a
// circle in two arcs, the second's range ending `overshoot` past 2*PI.
TopoDS_Face plate_of_arc_holes(int n, double overshoot) {
  const double               size = 2.5 * n;
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(0, 0, 0), gp_Pnt(size, 0, 0), gp_Pnt(size, size, 0), gp_Pnt(0, size, 0),
                                     true);
  BRepBuilderAPI_MakeFace face(gp_Pln(), outline.Wire());
  BRep_Builder            builder;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      const occ::handle<Geom_Circle> circle =
          new Geom_Circle(gp_Ax2(gp_Pnt(1.25 + 2.5 * i, 1.25 + 2.5 * j, 0), gp::DZ()), 0.5);
      const TopoDS_Edge first  = BRepBuilderAPI_MakeEdge(circle, 0.0, test::kPi);
      TopoDS_Edge       second = BRepBuilderAPI_MakeEdge(circle, test::kPi, 2.0 * test::kPi);
      builder.Range(second, test::kPi, 2.0 * test::kPi + overshoot);
      BRepBuilderAPI_MakeWire wire(first, second);
      CHECK(wire.IsDone());
      face.Add(TopoDS::Wire(wire.Wire().Reversed()));
    }
  }
  CHECK(face.IsDone());
  return face.Face();
}

// The seconds a fresh check takes to classify the face's wires; the
// classification must find no fault.
double seconds_to_classify(const TopoDS_Face& face) {
  const occ::handle<BRepCheck_Face> check  = new BRepCheck_Face(face);
  const auto                        start  = std::chrono::steady_clock::now();
  const BRepCheck_Status            status = check->ClassifyWires();
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  CHECK(status == BRepCheck_NoError);
  return seconds;
}

// The bound: 900 holes of arcs ending 4e-15 past 2*PI are classified within
// six times the time of the same holes with arcs ending at 2*PI (about 1
// with the patch, about 25 without it; the measurements are in the header).
void test_arc_holes_past_their_range_classified_as_fast_as_exact_ones() {
  const int         n     = 30;
  const TopoDS_Face past  = plate_of_arc_holes(n, 4e-15);
  const TopoDS_Face exact = plate_of_arc_holes(n, 0.0);
  CHECK(port::count_of(past, TopAbs_WIRE) == n * n + 1);
  CHECK(port::count_of(exact, TopAbs_WIRE) == n * n + 1);
  // The shortest of five interleaved runs of each, so that a burst of
  // load on one run does not decide the ratio.
  double past_seconds  = 1e300;
  double exact_seconds = 1e300;
  for (int round = 0; round < 5; ++round) {
    past_seconds  = std::min(past_seconds, seconds_to_classify(past));
    exact_seconds = std::min(exact_seconds, seconds_to_classify(exact));
  }
  const double ratio = past_seconds / exact_seconds;
  std::printf("classification of 900 holes of arcs: %.3f s past 2*PI, %.3f s at 2*PI, ratio %.2f\n", past_seconds,
              exact_seconds, ratio);
  CHECK(ratio < 6.0);
}

} // namespace

MITCAD_PORT_TEST("0120", test_arc_holes_past_their_range_classified_as_fast_as_exact_ones);
