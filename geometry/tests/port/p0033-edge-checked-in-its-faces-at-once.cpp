// SPDX-License-Identifier: MIT
// Port patch 0033: in the parallel mode of BRepCheck_Analyzer the faces are
// checked on several threads, and each face checks its edges in its
// context (BRepCheck_Edge::InContext), so an edge is checked in the
// contexts of its faces at the same time. Each context compared the face's
// curve with the edge's one curve adaptor (myHCurve, made by Minimum()).
// An adaptor of a B-spline keeps the polynomial of the span it evaluated
// last (its BSplCLib_Cache) and builds it again for a parameter outside
// that span: threads sharing one rebuild it for their own spans while the
// others evaluate with it, and a point comes out of another span's
// polynomial, far off the curve. The edge is then found off its face
// (BRepCheck_InvalidCurveOnSurface). The patch gives each context its own
// copy of the adaptor (ShallowCopy(), which copies no cache) in the
// parallel mode; the sequential mode is unchanged.
//
// The body: one edge, a planar cubic B-spline over [0, 22] with a simple
// knot at every k + 0.5 (23 spans; 26 poles zig-zagging 3 either side of
// the x axis), and five faces that extrude it in five directions
// (BRepPrimAPI_MakePrism without a copy keeps the edge; each face adds its
// p-curve, the line v = 0 of its extrusion surface, so the edge is same
// parameter on all of them). In each round a new BRepCheck_Edge in the
// parallel mode checks the edge in the first face alone (without the
// patch that builds the shared adaptor's cache, so the threads never race
// on creating it), then in the four others at once, on four threads
// released together and each delayed by its own 0 to 8 microseconds,
// which varies from round to round (a check takes some tens of
// microseconds), so that the threads are at different spans. The check
// (BRepLib_ValidateEdge, approximate method, both curves at the same
// parameters) evaluates the edge at 0, 1, ..., 22: the two ends without
// the cache (GeomAdaptor_Curve::IsBoundary), the 21 others each in a span
// of its own, so that every one of them rebuilds the cache. Without the
// patch the four threads rebuild the one cache for their own spans while
// the others read it, and a context finds the edge millimetres off its
// face (its tolerance is 1e-7): BRepCheck_InvalidCurveOnSurface, within the
// first rounds on a machine with two or more cores. With the patch every
// context is valid in all 2000 rounds.
//
// The boundary cases, valid with and without the patch: the same edge
// checked in the four faces one after the other in the sequential mode
// (the adaptor is shared, but by one thread at a time), and a B-spline of
// one span checked on four threads at once as above (the first face's
// check builds the cache of its only span, and the others only read it).

#include <atomic>
#include <chrono>
#include <cstdio>
#include <thread>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepCheck_Edge.hxx>
#include <BRepCheck_Status.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kLength = 22.0; // the parameter range, [0, 22]
constexpr int    kRounds = 2000;

// The planar cubic zig-zag over [0, 22]: with a simple knot at every
// k + 0.5 (k = 1 .. 22, 23 spans), or with none (one span).
occ::handle<Geom_BSplineCurve> zigzag(bool one_span) {
  const int                  inner = one_span ? 0 : 22;
  NCollection_Array1<double> knots(1, inner + 2);
  NCollection_Array1<int>    mults(1, inner + 2);
  knots(1) = 0.0;
  mults(1) = 4;
  for (int k = 1; k <= inner; ++k) {
    knots(k + 1) = k - 0.5;
    mults(k + 1) = 1;
  }
  knots(inner + 2) = kLength;
  mults(inner + 2) = 4;
  const int                  count = inner + 4;
  NCollection_Array1<gp_Pnt> poles(1, count);
  for (int i = 1; i <= count; ++i) {
    poles(i) = gp_Pnt(kLength * (i - 1) / (count - 1), i % 2 == 0 ? 3.0 : -3.0, 0.0);
  }
  return new Geom_BSplineCurve(poles, knots, mults, 3);
}

// The edge and five faces that extrude it, each keeping it as its edge.
struct Fan {
  TopoDS_Edge              edge;
  std::vector<TopoDS_Face> faces;
};

Fan fan_of(bool one_span) {
  Fan fan;
  fan.edge = BRepBuilderAPI_MakeEdge(zigzag(one_span));
  const gp_Vec directions[] = {gp_Vec(0.0, 0.0, 5.0), gp_Vec(0.0, 1.5, 5.0), gp_Vec(0.0, -1.5, 5.0),
                               gp_Vec(1.0, 0.0, -5.0), gp_Vec(-1.0, 1.5, -5.0)};
  for (const gp_Vec& direction : directions) {
    BRepPrimAPI_MakePrism prism(fan.edge, direction, false, false);
    TopExp_Explorer       face(prism.Shape(), TopAbs_FACE);
    if (face.More()) {
      fan.faces.push_back(TopoDS::Face(face.Current()));
    }
  }
  return fan;
}

// One round: a new check of the edge in the first face, then in the
// others, on threads at once (parallel) or one after the other. Whether
// every context is valid.
bool round_valid(const Fan& fan, bool parallel, int round) {
  const occ::handle<BRepCheck_Edge> check = new BRepCheck_Edge(fan.edge);
  check->SetParallel(parallel);
  check->InContext(fan.faces[0]);
  bool threw = false;
  if (parallel) {
    std::atomic<int>         ready{0};
    std::atomic<bool>        go{false};
    std::atomic<bool>        failed{false};
    std::vector<std::thread> threads;
    for (std::size_t k = 1; k < fan.faces.size(); ++k) {
      const std::chrono::nanoseconds delay((round * 389 + static_cast<int>(k) * 1031) % 8000);
      threads.emplace_back([&, k, delay] {
        ready.fetch_add(1);
        while (!go.load()) {
          std::this_thread::yield();
        }
        const auto until = std::chrono::steady_clock::now() + delay;
        while (std::chrono::steady_clock::now() < until) {
        }
        try {
          check->InContext(fan.faces[k]);
        } catch (...) {
          failed = true;
        }
      });
    }
    while (ready.load() < static_cast<int>(threads.size())) {
      std::this_thread::yield();
    }
    go = true;
    for (std::thread& thread : threads) {
      thread.join();
    }
    threw = failed.load();
  } else {
    for (std::size_t k = 1; k < fan.faces.size(); ++k) {
      try {
        check->InContext(fan.faces[k]);
      } catch (const Standard_Failure&) {
        threw = true;
      }
    }
  }
  if (threw) {
    std::printf("round %d: a check threw\n", round);
    return false;
  }
  for (std::size_t k = 0; k < fan.faces.size(); ++k) {
    const NCollection_List<BRepCheck_Status>& statuses = check->StatusOnShape(fan.faces[k]);
    if (statuses.Extent() != 1 || statuses.First() != BRepCheck_NoError) {
      std::printf("round %d: the edge in face %d has status %d\n", round, static_cast<int>(k),
                  statuses.IsEmpty() ? -1 : static_cast<int>(statuses.First()));
      return false;
    }
  }
  return true;
}

// The first round with an invalid context, -1 when all are valid.
int first_invalid_round(const Fan& fan, bool parallel, int rounds) {
  for (int round = 0; round < rounds; ++round) {
    if (!round_valid(fan, parallel, round)) {
      return round;
    }
  }
  return -1;
}

// The edge of 23 spans checked in four faces at once: without the patch
// the contexts share the adaptor's cache and one finds the edge off its
// face.
void test_edge_of_many_spans_checked_in_its_faces_at_once() {
  const Fan fan = fan_of(false);
  CHECK(fan.faces.size() == 5);
  if (fan.faces.size() != 5) {
    return;
  }
  const int invalid = first_invalid_round(fan, true, kRounds);
  std::printf("23 spans, parallel: first invalid round %d of %d\n", invalid, kRounds);
  CHECK(invalid == -1);
}

// Boundary: the same edge in the sequential mode, one face after another.
void test_edge_of_many_spans_checked_one_face_after_another() {
  const Fan fan = fan_of(false);
  CHECK(fan.faces.size() == 5);
  if (fan.faces.size() != 5) {
    return;
  }
  const int invalid = first_invalid_round(fan, false, 20);
  std::printf("23 spans, sequential: first invalid round %d of 20\n", invalid);
  CHECK(invalid == -1);
}

// Boundary: an edge of one span checked in four faces at once; its cache
// is built once and only read.
void test_edge_of_one_span_checked_in_its_faces_at_once() {
  const Fan fan = fan_of(true);
  CHECK(fan.faces.size() == 5);
  if (fan.faces.size() != 5) {
    return;
  }
  const int invalid = first_invalid_round(fan, true, kRounds);
  std::printf("one span, parallel: first invalid round %d of %d\n", invalid, kRounds);
  CHECK(invalid == -1);
}

} // namespace

MITCAD_PORT_TEST("0033", test_edge_of_many_spans_checked_in_its_faces_at_once);
MITCAD_PORT_TEST("0033", test_edge_of_many_spans_checked_one_face_after_another);
MITCAD_PORT_TEST("0033", test_edge_of_one_span_checked_in_its_faces_at_once);
