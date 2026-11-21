// SPDX-License-Identifier: MIT
// Port patch 0091: TopOpeBRepBuild_Builder::BuildEdges counts, in an array
// indexed 0 to NbPoints, the interferences of the data structure's curves
// and edges that use each point, and removes the points no interference
// uses. Of its four increments only one, a curve interference's geometry
// of kind POINT, checked its index (against NbPoints); the three others (a
// curve interference's support, an edge interference's geometry and an
// edge interference's support, each of kind POINT) incremented the array
// wherever the index pointed. An interference whose index names no point
// (the fillet's double interferences made one, with a vertex's index,
// before patch 0090) wrote outside the array: a heap overflow. The patch
// counts only the indices 1 to NbPoints.
//
// With patch 0090 no fillet, chamfer or boolean makes such an
// interference any more (0090 removed the only known source), so no body
// reaches the changed lines; the test builds the data structure itself and
// calls BuildEdges on it. The data structure: two points (the first used
// by the edge's interference of kind POINT, the second by none), an edge,
// a curve without geometry (BuildEdges builds no edge of it), and one
// interference with the index INT_MAX in one of the four places. Without
// the patch the increment reads and writes the int at 4 x INT_MAX bytes,
// 8 GB, past the start of a small array on the heap, memory a test
// process has not mapped on Linux, macOS or Windows: a segmentation fault
// (an access violation), which run_operation (guard.hpp, with OCCT's
// handlers installed by catch_occt_crashes) turns into the error
// "count: ..." (or Standard_OutOfRange from an OCCT built with its range
// checks). With the patch BuildEdges returns, keeps the used point and
// removes the unused one. One test per patched increment.
//
// The boundary cases, the same with and without the patch: INT_MAX on a
// curve interference's geometry, the increment OCCT already checked, and
// the data structure without an index outside the points.

#include <climits>
#include <cstdio>
#include <stdexcept>
#include <string>

#ifndef _WIN32
#include <signal.h>
#endif

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <NCollection_List.hxx>
#include <TopOpeBRepBuild_Builder.hxx>
#include <TopOpeBRepDS_BuildTool.hxx>
#include <TopOpeBRepDS_Curve.hxx>
#include <TopOpeBRepDS_DataStructure.hxx>
#include <TopOpeBRepDS_HDataStructure.hxx>
#include <TopOpeBRepDS_Interference.hxx>
#include <TopOpeBRepDS_Kind.hxx>
#include <TopOpeBRepDS_Point.hxx>
#include <TopOpeBRepDS_Transition.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/guard.hpp"
#include "port_test.hpp"

namespace {

// Where the interference with the index outside the points is.
enum class Outside { Nowhere, CurveGeometry, CurveSupport, EdgeGeometry, EdgeSupport };

constexpr int kOutside = INT_MAX;

// What BuildEdges did: its error (empty when it returned) and which points
// it kept.
struct Counted {
  std::string error;
  bool        used_kept   = false;
  bool        unused_kept = true;
};

occ::handle<TopOpeBRepDS_Interference> interference(TopOpeBRepDS_Kind support_kind, int support,
                                                    TopOpeBRepDS_Kind geometry_kind, int geometry) {
  return new TopOpeBRepDS_Interference(TopOpeBRepDS_Transition(), support_kind, support, geometry_kind,
                                       geometry);
}

// On Linux and macOS OCCT's handler jumps out of a fault with the signal
// still blocked on the thread; a fault of an earlier case would then end
// the process. Unblocks it (nothing to do on Windows).
void unblock_segv() {
#ifndef _WIN32
  sigset_t set;
  sigemptyset(&set);
  sigaddset(&set, SIGSEGV);
  sigaddset(&set, SIGBUS);
  pthread_sigmask(SIG_UNBLOCK, &set, nullptr);
#endif
}

Counted count_point_uses(Outside outside) {
  const occ::handle<TopOpeBRepDS_HDataStructure> hds = new TopOpeBRepDS_HDataStructure();
  TopOpeBRepDS_DataStructure&                    ds  = hds->ChangeDS();
  const int used   = ds.AddPoint(TopOpeBRepDS_Point(gp_Pnt(2.0, 0.0, 0.0), 1e-7));
  const int unused = ds.AddPoint(TopOpeBRepDS_Point(gp_Pnt(8.0, 0.0, 0.0), 1e-7));
  const TopoDS_Edge edge  = BRepBuilderAPI_MakeEdge(gp_Pnt(0.0, 0.0, 0.0), gp_Pnt(10.0, 0.0, 0.0));
  const int         ie    = ds.AddShape(edge);
  TopOpeBRepDS_Curve curve;
  const int          ic = ds.AddCurve(curve);
  NCollection_List<occ::handle<TopOpeBRepDS_Interference>>& on_edge  = ds.ChangeShapeInterferences(ie);
  NCollection_List<occ::handle<TopOpeBRepDS_Interference>>& on_curve = ds.ChangeCurveInterferences(ic);
  on_edge.Append(interference(TopOpeBRepDS_FACE, 0, TopOpeBRepDS_POINT, used));
  switch (outside) {
  case Outside::Nowhere:
    break;
  case Outside::CurveGeometry:
    on_curve.Append(interference(TopOpeBRepDS_FACE, 0, TopOpeBRepDS_POINT, kOutside));
    break;
  case Outside::CurveSupport:
    on_curve.Append(interference(TopOpeBRepDS_POINT, kOutside, TopOpeBRepDS_VERTEX, 0));
    break;
  case Outside::EdgeGeometry:
    on_edge.Append(interference(TopOpeBRepDS_FACE, 0, TopOpeBRepDS_POINT, kOutside));
    break;
  case Outside::EdgeSupport:
    on_edge.Append(interference(TopOpeBRepDS_POINT, kOutside, TopOpeBRepDS_VERTEX, 0));
    break;
  }

  mitcad::geometry::catch_occt_crashes();
  unblock_segv();
  Counted counted;
  try {
    mitcad::geometry::run_operation("count", [&hds] {
      TopOpeBRepBuild_Builder builder{TopOpeBRepDS_BuildTool()};
      builder.BuildEdges(hds);
    });
  } catch (const std::runtime_error& failure) {
    counted.error = failure.what();
  }
  if (counted.error.empty()) {
    counted.used_kept   = ds.KeepPoint(used);
    counted.unused_kept = ds.KeepPoint(unused);
  }
  return counted;
}

// The points counted as the patch counts them: no error, the used point
// kept, the unused one removed.
void check_counted(const char* where, Outside outside) {
  const Counted counted = count_point_uses(outside);
  std::printf("%s: %s, used point %s, unused point %s\n", where,
              counted.error.empty() ? "returned" : counted.error.c_str(),
              counted.used_kept ? "kept" : "removed", counted.unused_kept ? "kept" : "removed");
  CHECK(counted.error.empty());
  CHECK(counted.used_kept);
  CHECK(!counted.unused_kept);
}

// A curve interference's support of kind POINT with an index past the
// points.
void test_curve_support_outside_the_points() { check_counted("curve support", Outside::CurveSupport); }

// An edge interference's geometry of kind POINT with an index past the
// points (the case patch 0090's source made).
void test_edge_geometry_outside_the_points() { check_counted("edge geometry", Outside::EdgeGeometry); }

// An edge interference's support of kind POINT with an index past the
// points.
void test_edge_support_outside_the_points() { check_counted("edge support", Outside::EdgeSupport); }

// Boundary: the index on a curve interference's geometry, the one
// increment OCCT already checked.
void test_curve_geometry_outside_the_points() { check_counted("curve geometry", Outside::CurveGeometry); }

// Boundary: every index names a point.
void test_all_indices_within_the_points() { check_counted("within", Outside::Nowhere); }

} // namespace

MITCAD_PORT_TEST("0091", test_curve_support_outside_the_points);
MITCAD_PORT_TEST("0091", test_edge_geometry_outside_the_points);
MITCAD_PORT_TEST("0091", test_edge_support_outside_the_points);
MITCAD_PORT_TEST("0091", test_curve_geometry_outside_the_points);
MITCAD_PORT_TEST("0091", test_all_indices_within_the_points);
