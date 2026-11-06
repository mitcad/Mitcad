// SPDX-License-Identifier: MIT
// Roundings that end where a boss's cylinder meets a plate's flat side
// tangentially (mitcad#1359), with the port's patches 1330 to 1332, and
// both rounded together (mitcad#1374), with patches 1400 to 1402.
//
// A sketch of two circles of radius 25, their centres 85 apart, joined by
// the two lines tangent to both: the first circle and the region between
// them extruded 15 (the plate), the second circle 30 (the boss), joined.
// The boss's cylinder is the plate's end, one face from the bottom up; the
// plate's flat sides go on tangentially into it along the vertical edges at
// x = 85, y = +-25, and the cylinder's seam runs up from one of their top
// vertices (where the sketch's circle starts).
//
// The plate's top edge along its side at y = 25 (a tangent chain round the
// plate's other end to the side at y = -25) is rounded up to the boss: its
// contact on the top meets the boss's foot short of the vertex, and its
// contact on the side reaches the boss's cylinder where the two are tangent,
// on their common edge below the vertex. OCCT took that point for the end
// of that edge prolonged past the vertex and added the edge's piece above
// it again: the faces' wires ran along it twice (an invalid result). The
// edge is now cut there, and where the cylinder's seam starts at the
// vertex the seam runs on down to the cut (patch 1331).
//
// The boss's foot on the top is rounded up to the plate's sides: they cut
// the rounding, growing up to it along the vertical edges prolonged (an
// OnSame end). At the vertex where the cylinder's seam starts, that
// prolongation runs along the seam from its end, and OCCT split it where
// it meets the seam, at its own start: a curve without length (patch 1330).
//
// Every rounding at 1, 3 and 6 is valid, its new faces named after the
// edges, and takes away (or adds) the material the exact shapes give, with
// the cylinder's seam at either vertex, through OCCT's fillet alone and
// through MitCad's fillet, alone and with the boss's top edge. The body as
// MitCad's sketch and extrusions build it is also the case
// `boss-tangent-side-rounding` of MITCAD_GEOMETRY_CASES (skipped without
// it).
//
// The plate's top edge and the boss's foot rounded together (mitcad#1374):
// the two blends meet at the vertices from opposite sides of the top (the
// side's takes material away below it, the foot's adds it above), and
// their contacts on the top cross at x = 85 - sqrt((25 + r)^2 - (25 - r)^2),
// y = +-(25 - r), short of the vertices. OCCT's corners joined the two into
// an invalid result; its fillet now reports them as overlapping (patch
// 1400), and MitCad's fillet rounds the side's edge first and the foot on
// that result, the foot's tangent chain running on along the side's blend's
// cut on the cylinder to where that blend vanishes, at the cylinder's seam
// on one side (patch 1401: at 1 the contact's p-curve ended a period off
// there) and without a sliver of micrometres at its ends (patch 1402: at 3
// and 6). The result is the one OCCT's fillet alone gives built so, one
// after the other.

#include <cmath>
#include <cstdio>
#include <sstream>
#include <cstdlib>
#include <fstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_Map.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/naming.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

std::string env(const char* name) {
#ifdef _MSC_VER
  char*       value  = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, name) == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? std::string(value) : std::string();
#endif
}

constexpr double kRadius = 25.0; // both circles
constexpr double kApart  = 85.0; // between their centres
constexpr double kPlate  = 15.0; // the plate's height
constexpr double kBoss   = 30.0; // the boss's height

const double kRadii[] = {1.0, 3.0, 6.0};

Segment arc(const std::string& name, double cx, double from, double to) {
  Segment s;
  s.name        = name;
  s.kind        = SegmentKind::Arc;
  s.center      = gp_Pnt2d(cx, 0.0);
  s.radius      = kRadius;
  s.start_angle = from;
  s.end_angle   = to;
  return s;
}

Segment line(const std::string& name, const gp_Pnt2d& a, const gp_Pnt2d& b) {
  Segment s;
  s.name  = name;
  s.kind  = SegmentKind::Line;
  s.start = a;
  s.end   = b;
  return s;
}

// A circle as its two halves split at the tangent lines c3 (y = 25) and c4
// (y = -25), as the sketch splits it: the half from the tangent point at
// y = 25 first (`top_first`), or the one from y = -25. The extrusion's
// cylinder has its seam where the first half starts.
Region disc(int curve, double cx, bool top_first) {
  const std::string c = "c" + std::to_string(curve);
  Region region;
  region.name = "r{" + c + "[c3,c4]," + c + "[c4,c3]}";
  Loop          loop;
  const Segment left  = arc(c + "[c3,c4]", cx, kPi / 2.0, 3.0 * kPi / 2.0);
  const Segment right = arc(c + "[c4,c3]", cx, -kPi / 2.0, kPi / 2.0);
  loop.segments.push_back(top_first ? left : right);
  loop.segments.push_back(top_first ? right : left);
  region.loops.push_back(loop);
  return region;
}

// The region between the circles and the lines.
Region middle() {
  Region region;
  region.name = "r{c4[c1,c2],c2[c3,c4],c3[c2,c1],c1[c4,c3]}";
  Loop loop;
  loop.segments.push_back(line("c4[c1,c2]", gp_Pnt2d(0.0, -kRadius), gp_Pnt2d(kApart, -kRadius)));
  loop.segments.push_back(arc("c2[c3,c4]", kApart, kPi / 2.0, 3.0 * kPi / 2.0));
  loop.segments.push_back(line("c3[c2,c1]", gp_Pnt2d(kApart, kRadius), gp_Pnt2d(0.0, kRadius)));
  loop.segments.push_back(arc("c1[c4,c3]", 0.0, -kPi / 2.0, kPi / 2.0));
  region.loops.push_back(loop);
  return region;
}

ShapePtr prism(const std::string& feature, const std::vector<Region>& regions, double height) {
  ExtrudeFeatureSpec spec;
  spec.feature        = feature;
  spec.regions        = regions;
  spec.side1.distance = height;
  return extrude_feature(spec);
}

// The plate (F1) and the boss (F2) joined.
ShapePtr plate_and_boss(bool top_first) {
  const ShapePtr      plate  = prism("F1", {disc(1, 0.0, top_first), middle()}, kPlate);
  const ShapePtr      boss   = prism("F2", {disc(2, kApart, top_first)}, kBoss);
  const BooleanResult joined = boolean(BooleanOp::Join, {plate.get()}, *boss);
  CHECK(joined.pieces.size() == 1);
  return joined.pieces.size() == 1 ? joined.pieces.front().shape : nullptr;
}

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-9, true);
  return props.Mass();
}

gp_Pnt middle_of(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
}

// The body's edge whose middle lies at the point, or -1.
int edge_at(const Shape& body, const gp_Pnt& point) {
  for (int e = 0; e < body.edge_count(); ++e) {
    if (middle_of(body.edge(e)).Distance(point) < 1.0e-6) {
      return e;
    }
  }
  return -1;
}

// The case's edges: the plate's top edge at y = 25, the boss's foot and the
// boss's top edge.
struct Edges {
  int side = -1;
  int foot = -1;
  int top  = -1;
};

Edges edges_of(const Shape& body) {
  Edges out;
  out.side = edge_at(body, gp_Pnt(kApart / 2.0, kRadius, kPlate));
  out.foot = edge_at(body, gp_Pnt(kApart - kRadius, 0.0, kPlate));
  out.top  = edge_at(body, gp_Pnt(kApart, kRadius, kBoss));
  if (out.top < 0) {
    out.top = edge_at(body, gp_Pnt(kApart, -kRadius, kBoss));
  }
  return out;
}

// The faces of the shape named so.
int count_named(const Shape& shape, const std::string& name) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& n : shape.face_names(f)) {
      count += n == name ? 1 : 0;
    }
  }
  return count;
}

// ---- The material the roundings take away or add, from exact shapes ----

// A rounding of radius r of the plate's top edge: along both sides the
// square at the corner less the ball's path, outside the boss; round the
// plate's far end the same section turned about its axis.
double side_rounding_removes(double r) {
  double total = 0.0;
  for (const double side : {1.0, -1.0}) {
    const double       y0     = side > 0.0 ? kRadius - r : -kRadius;
    const TopoDS_Shape corner = BRepPrimAPI_MakeBox(gp_Pnt(0.0, y0, kPlate - r), gp_Pnt(kApart, y0 + r, kPlate)).Shape();
    const TopoDS_Shape ball =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1.0, side * (kRadius - r), kPlate - r), gp_Dir(1.0, 0.0, 0.0)), r,
                               kApart + 2.0)
        .Shape();
    const TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kApart, 0.0, -1.0), gp_Dir(0.0, 0.0, 1.0)), kRadius, kBoss + 2.0).Shape();
    total += volume_of(BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(corner, ball).Shape(), boss).Shape());
  }
  const gp_Ax2       axis(gp_Pnt(0.0, 0.0, kPlate - r), gp_Dir(0.0, 0.0, 1.0));
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(axis, kRadius, r).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(axis, kRadius - r, r).Shape();
  const TopoDS_Shape torus = BRepPrimAPI_MakeTorus(axis, kRadius - r, r).Shape();
  const TopoDS_Shape far_half =
    BRepPrimAPI_MakeBox(gp_Pnt(-kRadius - 1.0, -kRadius - 1.0, kPlate - r - 1.0), gp_Pnt(0.0, kRadius + 1.0, kPlate + 1.0))
      .Shape();
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(outer, inner).Shape(), torus).Shape();
  return total + volume_of(BRepAlgoAPI_Common(ring, far_half).Shape());
}

// A rounding of radius r of the boss's foot adds the ring round the boss
// between radii 25 and 25 + r on the plate's top less the torus the ball
// sweeps, on the plate's side (x < 85) and between its sides (|y| < 25),
// which cut it.
double foot_rounding_adds(double r) {
  const gp_Ax2       axis(gp_Pnt(kApart, 0.0, kPlate), gp_Dir(0.0, 0.0, 1.0));
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(axis, kRadius + r, r).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(axis, kRadius, r).Shape();
  const TopoDS_Shape torus =
    BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(kApart, 0.0, kPlate + r), gp_Dir(0.0, 0.0, 1.0)), kRadius + r, r).Shape();
  const TopoDS_Shape within =
    BRepPrimAPI_MakeBox(gp_Pnt(kApart - kRadius - r - 1.0, -kRadius, kPlate - 1.0), gp_Pnt(kApart, kRadius, kPlate + r + 1.0))
      .Shape();
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(outer, inner).Shape(), torus).Shape();
  return volume_of(BRepAlgoAPI_Common(ring, within).Shape());
}

// A rounding of radius r of the boss's top edge: the square at the rim less
// the torus, all round.
double top_rounding_removes(double r) {
  const gp_Ax2       axis(gp_Pnt(kApart, 0.0, kBoss - r), gp_Dir(0.0, 0.0, 1.0));
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(axis, kRadius, r).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(axis, kRadius - r, r).Shape();
  const TopoDS_Shape torus = BRepPrimAPI_MakeTorus(axis, kRadius - r, r).Shape();
  return volume_of(BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(outer, inner).Shape(), torus).Shape());
}

// ---- The roundings ----

// The change of volume (removed; negative for added) within a relative
// 1e-6 of the exact one.
void check_removed(const std::string& label, double r, double removed, double expected) {
  std::printf("  %s %.0f: removed %.6f (exact %.6f)\n", label.c_str(), r, removed, expected);
  CHECK(std::abs(removed - expected) <= 1.0e-6 * std::abs(expected));
}

// Adds the edges of a copy of the body to OCCT's fillet with radius r, the
// contours stopping where their edges turn as in MitCad's fillet (port
// patch 0180), and builds it; false where it throws.
bool occt_build(BRepFilletAPI_MakeFillet& round, const Shape& copy, const std::vector<int>& edges, double r) {
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const int e : edges) {
    selected.Add(copy.edge(e));
  }
  round.StopChainsAtTurns(selected);
  for (const int e : edges) {
    round.Add(r, copy.edge(e));
  }
  try {
    OCC_CATCH_SIGNALS
    round.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("  OCCT %.0f: %s\n", r, failure.what());
    return false;
  }
  return true;
}

// OCCT's fillet alone of the edges with radius r on a copy of the body
// (OCCT's fillet adds curves to its input's edges); null where it fails or
// its result is not valid.
TopoDS_Shape occt_rounded(const Shape& body, const std::vector<int>& edges, double r) {
  const ShapePtr           copy = import_body(BRepBuilderAPI_Copy(body.occt()).Shape(), "F9", 0);
  BRepFilletAPI_MakeFillet round(copy->occt());
  if (!occt_build(round, *copy, edges, r)) {
    return TopoDS_Shape();
  }
  if (!round.IsDone()) {
    std::printf("  OCCT %.0f: not done\n", r);
    return TopoDS_Shape();
  }
  if (!BRepCheck_Analyzer(round.Shape()).IsValid()) {
    std::printf("  OCCT %.0f: not valid\n", r);
    return TopoDS_Shape();
  }
  return round.Shape();
}

// MitCad's fillet (F3) of the edges with radius r; null where it fails.
ShapePtr rounded(const Shape& body, const std::vector<int>& edges, double r) {
  FilletSet set;
  for (const int e : edges) {
    set.edges.push_back(body.edge_name(e));
  }
  set.radius = r;
  try {
    return fillet("F3", body, {set});
  } catch (const std::exception& failure) {
    std::printf("  MitCad %.0f: %s\n", r, failure.what());
    return nullptr;
  }
}

// The faces of the shape named so, or so with a piece's number ("#1").
int count_named_pieces(const Shape& shape, const std::string& name) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& n : shape.face_names(f)) {
      count += n == name || n.rfind(name + "#", 0) == 0 ? 1 : 0;
    }
  }
  return count;
}

// Valid, without notes, every face named and none a sliver (an area of at
// least 1e-3), the rounding of each edge named after it: in one face, or
// with `pieces` in one or more (a tangent chain's pieces that OCCT's
// history gives the edge, numbered).
void check_result(const Shape& body, const ShapePtr& result, const std::vector<int>& edges, bool pieces = false) {
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(result->notes().empty());
  for (const std::string& note : result->notes()) {
    std::printf("  note: %s\n", note.c_str());
  }
  for (int f = 0; f < result->face_count(); ++f) {
    CHECK(!result->face_names(f).empty());
    GProp_GProps props;
    BRepGProp::SurfaceProperties(result->face(f), props);
    if (props.Mass() < 1.0e-3) {
      std::printf("  a sliver: face %d, area %.3g\n", f, props.Mass());
    }
    CHECK(props.Mass() >= 1.0e-3);
  }
  for (const int e : edges) {
    const std::string name  = face_name("F3", "fillet", body.edge_name(e));
    const int         named = pieces ? count_named_pieces(*result, name) : count_named(*result, name);
    if (pieces ? named < 1 : named != 1) {
      std::printf("  %d face(s) named %s\n", named, name.c_str());
    }
    CHECK(pieces ? named >= 1 : named == 1);
  }
}

// The roundings of the case's edges (the side's top edge, the foot, the
// boss's top edge, as asked) with every radius, through OCCT's fillet alone
// and through MitCad's fillet.
void check_roundings(const std::string& label, const Shape& body, const Edges& at, bool side, bool foot,
                     bool top) {
  std::vector<int> edges;
  if (side) {
    edges.push_back(at.side);
  }
  if (foot) {
    edges.push_back(at.foot);
  }
  if (top) {
    edges.push_back(at.top);
  }
  const double before = volume_of(body.occt());
  for (const double r : kRadii) {
    const double expected = (side ? side_rounding_removes(r) : 0.0) - (foot ? foot_rounding_adds(r) : 0.0) +
                            (top ? top_rounding_removes(r) : 0.0);
    const TopoDS_Shape alone = occt_rounded(body, edges, r);
    CHECK(!alone.IsNull());
    if (!alone.IsNull()) {
      check_removed(label + " OCCT", r, before - volume_of(alone), expected);
    }
    const ShapePtr result = rounded(body, edges, r);
    check_result(body, result, edges);
    if (result) {
      check_removed(label + " MitCad", r, before - volume_of(result->occt()), expected);
    }
  }
}

// ---- The side's top edge and the foot together (mitcad#1374) ----

// OCCT's fillet alone of the edges together builds nothing and reports the
// side's and the foot's roundings, the two faulty contours, as overlapping
// (port patch 1400).
void check_overlap_reported(const Shape& body, const std::vector<int>& edges, double r) {
  const ShapePtr           copy = import_body(BRepBuilderAPI_Copy(body.occt()).Shape(), "F9", 0);
  BRepFilletAPI_MakeFillet round(copy->occt());
  CHECK(occt_build(round, *copy, edges, r));
  CHECK(!round.IsDone());
  CHECK(round.Failure() == ChFi3d_Builder::Failure_Overlap);
  CHECK(round.NbFaultyContours() == 2);
  std::printf("  OCCT %.0f together: done %d, failure %d, %d faulty contour(s)\n", r, round.IsDone() ? 1 : 0,
              static_cast<int>(round.Failure()), round.NbFaultyContours());
}

// OCCT's fillet alone of the convex edges (the side's top edge, with the
// boss's top edge where asked) and then of the foot on that result, found
// again at its middle: the two built one after the other; null where a step
// fails.
TopoDS_Shape occt_one_after_the_other(const Shape& body, const Edges& at, bool top, double r) {
  std::vector<int> convex = {at.side};
  if (top) {
    convex.push_back(at.top);
  }
  const TopoDS_Shape first = occt_rounded(body, convex, r);
  if (first.IsNull()) {
    return TopoDS_Shape();
  }
  const ShapePtr step = import_body(first, "F8", 0);
  const int      foot = edge_at(*step, gp_Pnt(kApart - kRadius, 0.0, kPlate));
  CHECK(foot >= 0);
  return foot >= 0 ? occt_rounded(*step, {foot}, r) : TopoDS_Shape();
}

// The side's top edge and the foot, with the boss's top edge where asked,
// rounded in one fillet with every radius: OCCT's fillet alone reports the
// overlap, and MitCad's fillet builds the result OCCT's fillet alone gives
// one after the other, valid and named (the foot's chain in numbered pieces:
// OCCT's history gives the walked piece at one end the foot's edge, the one
// at the other end the cut it runs along there). That result differs from the
// roundings' own material (the exact shapes above) only near the vertices,
// within the boxes where the contacts on the top cross short of them.
void check_together(const std::string& label, const Shape& body, const Edges& at, bool top) {
  std::vector<int> edges = {at.side, at.foot};
  if (top) {
    edges.push_back(at.top);
  }
  const double before = volume_of(body.occt());
  for (const double r : kRadii) {
    check_overlap_reported(body, edges, r);
    const TopoDS_Shape reference = occt_one_after_the_other(body, at, top, r);
    CHECK(!reference.IsNull());
    const ShapePtr result = rounded(body, edges, r);
    check_result(body, result, edges, true);
    if (reference.IsNull()) {
      continue;
    }
    const double expected = before - volume_of(reference);
    if (result) {
      check_removed(label + " MitCad", r, before - volume_of(result->occt()), expected);
    }
    const double own =
      side_rounding_removes(r) - foot_rounding_adds(r) + (top ? top_rounding_removes(r) : 0.0);
    const double reach = std::sqrt((kRadius + r) * (kRadius + r) - (kRadius - r) * (kRadius - r));
    std::printf("  %s %.0f: one after the other %.6f, the roundings' own %.6f\n", label.c_str(), r, expected,
                own);
    CHECK(std::abs(expected - own) <= 2.0 * reach * r * 2.0 * r);
  }
}

void check_body(const Shape& body) {
  CHECK(BRepCheck_Analyzer(body.occt()).IsValid());
  const Edges at = edges_of(body);
  CHECK(at.side >= 0 && at.foot >= 0 && at.top >= 0);
  if (at.side < 0 || at.foot < 0 || at.top < 0) {
    return;
  }
  check_roundings("side", body, at, true, false, false);
  check_roundings("foot", body, at, false, true, false);
  check_roundings("side and top", body, at, true, false, true);
  check_roundings("foot and top", body, at, false, true, true);
  check_together("side and foot", body, at, false);
  check_together("side, foot and top", body, at, true);
}

// The cylinder's seam at the end of the side rounded (y = 25), as the
// sketch makes it.
void test_seam_at_the_side_rounded() {
  const ShapePtr body = plate_and_boss(true);
  CHECK(body != nullptr);
  if (body) {
    check_body(*body);
  }
}

// The seam at the other side's end (y = -25).
void test_seam_at_the_other_side() {
  const ShapePtr body = plate_and_boss(false);
  CHECK(body != nullptr);
  if (body) {
    check_body(*body);
  }
}

// The body as MitCad's sketch and extrusions build it, from
// MITCAD_GEOMETRY_CASES: the edges `edges.txt` lists by their indices (the
// side's top edge, the foot and the boss's top edge) are the ones found.
void test_case_body() {
  const std::string root = env("MITCAD_GEOMETRY_CASES");
  if (root.empty()) {
    std::printf("test_case_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string folder = root + "/boss-tangent-side-rounding/";
  TopoDS_Shape      shape;
  BRep_Builder      builder;
  if (!BRepTools::Read(shape, (folder + "body.brep").c_str(), builder) || shape.IsNull()) {
    std::printf("test_case_body: skipped (no boss-tangent-side-rounding in MITCAD_GEOMETRY_CASES)\n");
    return;
  }
  const ShapePtr   body = import_body(shape, "F1", 0);
  std::vector<int> listed;
  std::ifstream    in(folder + "edges.txt");
  std::string      text;
  while (std::getline(in, text)) {
    std::istringstream fields(text);
    int                index = -1;
    if (fields >> index) {
      listed.push_back(index);
    }
  }
  const Edges at = edges_of(*body);
  CHECK(listed.size() == 3);
  if (listed.size() == 3) {
    CHECK(listed[0] == at.side);
    CHECK(listed[1] == at.foot);
    CHECK(listed[2] == at.top);
  }
  check_body(*body);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_seam_at_the_side_rounded", test::test_seam_at_the_side_rounded);
  test::guarded("test_seam_at_the_other_side", test::test_seam_at_the_other_side);
  test::guarded("test_case_body", test::test_case_body);
  if (test::failures != 0) {
    std::printf("%d check(s) failed\n", test::failures);
  }
  return test::failures == 0 ? 0 : 1;
}
