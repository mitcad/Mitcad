// SPDX-License-Identifier: MIT
// Port patch 0023: ChFi3d_IntTraces intersects the contact lines of two
// stripes ending at one vertex on their common face. Among several
// intersections it kept, for a periodic contact line (a rounding along a
// circle), the one whose parameter lay nearest the end of that stripe's
// surface data, which may lie far past the vertex on the stripe's
// extension. Two roundings meeting at a shallow corner then met at the far
// crossing of the arc's contact circle with the line's contact line, and
// the bevel between them (PerformTwoCornerbyInter) was a self-intersecting
// face. Where the vertex is given and the other, non-periodic contact line
// prefers the other crossing, the crossing nearest the vertex is kept.
//
// The body: a plate 5 thick whose outline runs along y = 0 up to the origin
// and turns there by 10 degrees (not tangent: 0.17 rad, past the 0.1 of
// OCCT's tangency test) into an arc of radius 40 bending the same way
// through 60 degrees. The top edges of the line and of the arc rounded with
// 2: on the top the line's contact is y = 2 and the arc's a circle of
// radius 38; they cross 0.16 behind the vertex and again 13.7 behind it, on
// the torus's extension 20 degrees back. The result is valid, has no face
// that intersects itself or another, and loses about the two roundings'
// sections along the line (30) and round the arc (Pappus, the section's
// centroid 0.2234 r inside the edge): 25.8 + 35.6 = 61.3.

#include <cmath>
#include <cstdio>

#include <BOPAlgo_ArgumentAnalyzer.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
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

struct Plate {
  TopoDS_Shape body;
  gp_Pnt       line_middle; // the middle of the line's top edge
  gp_Pnt       arc_middle;  // the middle of the arc's top edge
};

// The plate whose outline turns by `turn` (radians) at the origin from the
// line y = 0 into the arc.
Plate plate(double turn) {
  const gp_Pnt  centre(-kRadius * std::sin(turn), kRadius * std::cos(turn), 0.0);
  const double  start = -0.5 * test::kPi + turn; // the origin, seen from the centre
  const double  end   = start + test::kPi / 3.0;
  const gp_Circ circle(gp_Ax2(centre, gp::DZ()), kRadius);
  const gp_Pnt  far(centre.X() + kRadius * std::cos(end), centre.Y() + kRadius * std::sin(end), 0.0);
  const gp_Pnt  a(-30.0, 0.0, 0.0), o(0.0, 0.0, 0.0), b(-30.0, far.Y(), 0.0);
  BRepBuilderAPI_MakeWire outline;
  outline.Add(BRepBuilderAPI_MakeEdge(a, o).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(circle, start, end).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(far, b).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(b, a).Edge());
  CHECK(outline.IsDone());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const double      mid  = 0.5 * (start + end);
  return {BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, kHeight)).Shape(), gp_Pnt(-15.0, 0.0, kHeight),
          gp_Pnt(centre.X() + kRadius * std::cos(mid), centre.Y() + kRadius * std::sin(mid), kHeight)};
}

// The shape's faces intersect neither themselves nor each other.
bool free_of_self_interference(const TopoDS_Shape& shape) {
  BOPAlgo_ArgumentAnalyzer analyzer;
  analyzer.SetShape1(shape);
  analyzer.SelfInterMode() = true;
  analyzer.Perform();
  return !analyzer.HasFaulty();
}

// The line's and the arc's top edges rounded with 2, meeting at the shallow
// corner: valid, no self-intersection, the volume of the two roundings.
void test_roundings_meet_near_a_shallow_corner() {
  const Plate                    p     = plate(10.0 * test::kPi / 180.0);
  const std::vector<TopoDS_Edge> edges = port::edges_where(p.body, [&p](const gp_Pnt& m) {
    return m.Distance(p.line_middle) < 1e-6 || m.Distance(p.arc_middle) < 1e-6;
  });
  CHECK(edges.size() == 2);
  const TopoDS_Shape rounded = port::rounded(p.body, edges, kRound);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK(free_of_self_interference(rounded));
  const double section  = kRound * kRound * (1.0 - test::kPi / 4.0);
  const double expected = 30.0 * section + test::kPi / 3.0 * (kRadius - 0.2234 * kRound) * section;
  const double removed  = port::volume_of(p.body) - port::volume_of(rounded);
  std::printf("shallow corner: removed %.4f, the roundings' sections %.4f\n", removed, expected);
  CHECK(std::abs(removed - expected) < 3.0);
}

} // namespace

MITCAD_PORT_TEST("0023", test_roundings_meet_near_a_shallow_corner);
