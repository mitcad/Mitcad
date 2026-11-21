// SPDX-License-Identifier: MIT
// Port patch 0023: ChFi3d_IntTraces intersects the contact lines of two
// stripes ending at one vertex on their common face. Among several
// intersections it kept, for a periodic contact line (a rounding along a
// circle), the one whose parameter lay nearest the end of that stripe's
// surface data, which may lie far past the vertex on the stripe's
// extension. Two roundings meeting at a shallow corner then met at the far
// crossing of the arc's contact circle with the line's contact line, and
// the corner between them (PerformTwoCornerbyInter) was built there. Where
// the vertex is given and the other, non-periodic contact line prefers the
// other crossing, the crossing nearest the vertex is kept.
//
// The body: a plate 5 thick whose outline runs along y = 0 up to the origin
// and turns there by 10 degrees (not tangent: 0.17 rad, past the 0.1 of
// OCCT's tangency test) into an arc of radius 40 bending the same way
// through 60 degrees (its centre at (-6.946, 39.392)). The top edges of the
// line and of the arc rounded with 2. On the top the line's contact is
// y = 2 and the arc's the circle of radius 38; they cross at x = -0.176
// (0.26 degrees into the arc) and again at x = -13.716 (20.26 degrees back
// from the vertex, on the torus's extension).
//
// Why the unpatched code takes the far crossing: the arc's contact circle
// stays on the top face back from the vertex until it leaves it across the
// plate's end x = -30, 47.3 degrees back, and SplitKPart keeps the arc's
// surface data on the top running that far (the face beyond its other
// contact at the vertex, the side y = 0, is not tangent to the arc's side,
// so SearchFace finds none and the top's domain is taken whole). The
// reference parameter of the arc's trace in ChFi3d_IntTraces is that end:
// the far crossing lies 27 degrees from it, the near one 47.6. The
// intersections come sorted by the parameter on the first trace
// (IntRes2d_Intersection::Insert); with the arc's stripe first (its edge
// added first) that is the arc's 2d circle, which starts at the start of
// the spine's edge and runs along it (ChFiDS_Spine::Circle,
// ChFiKPart_MakeFillet): the near crossing comes first either way the
// spine runs (0.26 and 339.74 degrees, or 59.74 and 80.26). The second, far
// crossing is then nearer the reference and replaces it. The line's trace
// prefers the near crossing (its end lies 7.26 past the vertex, where y = 2
// leaves the top across the arc), so with the patch the vertex decides.
//
// Met at the far crossing, the line's rounding is cut 13.7 back from the
// vertex and the arc's torus runs on in its place: on the top its contact
// lies on the radius 38 circle, 0.3 to 0.6 nearer the side than the line's
// y = 2. A point 0.8 from the side and 0.25 below the top, 4 to 10 back from
// the vertex, is then inside the torus's tube (1.85 to 1.89 from its
// circle, radius 2): material is left there. The line's rounding removes
// it (2.12 from its axis). So the check that fails without the patch: the
// result is valid and those points are outside it. The boundary: the same
// plate only 10 long, whose line's contact ends before the far crossing
// (13.7 back): one crossing, the same result with and without the patch.

#include <cmath>
#include <cstdio>

#include <BOPAlgo_ArgumentAnalyzer.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 40.0; // the outline's arc
constexpr double kRound  = 2.0;  // the roundings
constexpr double kHeight = 5.0;
constexpr double kTurn   = 10.0 * test::kPi / 180.0;

struct Plate {
  TopoDS_Shape body;
  TopoDS_Edge  line_top; // the line's top edge
  TopoDS_Edge  arc_top;  // the arc's top edge
};

// The edge of `shape` whose middle lies at `p`; null unless there is one.
TopoDS_Edge edge_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  const std::vector<TopoDS_Edge> edges =
    port::edges_where(shape, [&p](const gp_Pnt& m) { return m.Distance(p) < 1e-6; });
  return edges.size() == 1 ? edges.front() : TopoDS_Edge();
}

// The plate whose outline runs along y = 0 from x = -length to the origin
// and turns there into the arc.
Plate plate(double length) {
  const gp_Pnt  centre(-kRadius * std::sin(kTurn), kRadius * std::cos(kTurn), 0.0);
  const double  start = -0.5 * test::kPi + kTurn; // the origin, seen from the centre
  const double  end   = start + test::kPi / 3.0;
  const gp_Circ circle(gp_Ax2(centre, gp::DZ()), kRadius);
  const gp_Pnt  far(centre.X() + kRadius * std::cos(end), centre.Y() + kRadius * std::sin(end), 0.0);
  const gp_Pnt  a(-length, 0.0, 0.0), o(0.0, 0.0, 0.0), b(-length, far.Y(), 0.0);
  BRepBuilderAPI_MakeWire outline;
  outline.Add(BRepBuilderAPI_MakeEdge(a, o).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(circle, start, end).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(far, b).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(b, a).Edge());
  CHECK(outline.IsDone());
  const TopoDS_Face  face = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const TopoDS_Shape body = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, kHeight)).Shape();
  const double       mid  = 0.5 * (start + end);
  return {body, edge_at(body, gp_Pnt(-0.5 * length, 0.0, kHeight)),
          edge_at(body, gp_Pnt(centre.X() + kRadius * std::cos(mid), centre.Y() + kRadius * std::sin(mid),
                               kHeight))};
}

// The two top edges rounded with 2, `first` added first (its stripe the
// first at the vertex).
TopoDS_Shape rounded_in_order(const TopoDS_Shape& body, const TopoDS_Edge& first, const TopoDS_Edge& second) {
  BRepFilletAPI_MakeFillet fillet(body);
  fillet.Add(kRound, first);
  fillet.Add(kRound, second);
  return port::built(fillet, "fillet");
}

// The shape's faces intersect neither themselves nor each other.
bool free_of_self_interference(const TopoDS_Shape& shape) {
  BOPAlgo_ArgumentAnalyzer analyzer;
  analyzer.SetShape1(shape);
  analyzer.SelfInterMode() = true;
  analyzer.Perform();
  return !analyzer.HasFaulty();
}

// The points 0.8 from the side y = 0 and 0.25 below the top at the given x
// (back from the vertex) that the line's rounding removes, and are outside
// `rounded`; prints those that are not.
int removed_by_the_line(const TopoDS_Shape& rounded, const std::vector<double>& xs, const char* what) {
  int outside = 0;
  for (const double x : xs) {
    const gp_Pnt                p(x, 0.8, kHeight - 0.25);
    BRepClass3d_SolidClassifier classifier(rounded, p, 1e-7);
    if (classifier.State() == TopAbs_OUT) {
      ++outside;
    } else {
      std::printf("%s: material left at (%.1f, 0.8, %.2f) under the line's rounding\n", what, x, p.Z());
    }
  }
  return outside;
}

// The line's and the arc's top edges rounded with 2, meeting at the shallow
// corner, each stripe first in turn: valid, no self-intersection, the line's
// rounding running on up to the near crossing (the points it removes 4 to
// 10 back from the vertex are outside), the volume of the two roundings.
void test_roundings_meet_near_a_shallow_corner() {
  const Plate p = plate(30.0);
  CHECK(!p.line_top.IsNull() && !p.arc_top.IsNull());
  if (p.line_top.IsNull() || p.arc_top.IsNull()) {
    return;
  }
  const double section  = kRound * kRound * (1.0 - test::kPi / 4.0);
  const double expected = 30.0 * section + test::kPi / 3.0 * (kRadius - 0.2234 * kRound) * section;
  for (const bool arc_first : {true, false}) {
    const char*        what    = arc_first ? "arc first" : "line first";
    const TopoDS_Shape rounded = arc_first ? rounded_in_order(p.body, p.arc_top, p.line_top)
                                           : rounded_in_order(p.body, p.line_top, p.arc_top);
    CHECK(port::valid(rounded));
    if (!port::valid(rounded)) {
      continue;
    }
    CHECK(free_of_self_interference(rounded));
    CHECK(removed_by_the_line(rounded, {-4.0, -7.0, -10.0}, what) == 3);
    const double removed = port::volume_of(p.body) - port::volume_of(rounded);
    std::printf("shallow corner, %s: removed %.4f, the roundings' sections %.4f\n", what, removed, expected);
    CHECK(std::abs(removed - expected) < 3.0);
  }
}

// The boundary: the plate only 10 long, the line's contact ending before
// the far crossing (13.7 back): one crossing, which both ways is the near
// one; the same checks.
void test_roundings_meet_at_the_only_crossing() {
  const Plate p = plate(10.0);
  CHECK(!p.line_top.IsNull() && !p.arc_top.IsNull());
  if (p.line_top.IsNull() || p.arc_top.IsNull()) {
    return;
  }
  const TopoDS_Shape rounded = rounded_in_order(p.body, p.arc_top, p.line_top);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK(free_of_self_interference(rounded));
  CHECK(removed_by_the_line(rounded, {-4.0, -7.0}, "short plate") == 2);
}

} // namespace

MITCAD_PORT_TEST("0023", test_roundings_meet_near_a_shallow_corner);
MITCAD_PORT_TEST("0023", test_roundings_meet_at_the_only_crossing);
