// SPDX-License-Identifier: MIT
// A chamfer whose chain ends against an earlier chamfer's face, built with
// OCCT's chamfer alone (mitcad#1185). A plate 52 x 40 x 12 (z 40 to 52), its
// two top long edges chamfered 2, then its far end cut round to a rounded
// octagon (straight sides on the plate's end and sides, 45 degree sides
// between, corners of radius 4 tangent to them). The cut leaves a chain of
// tangent top edges: a diagonal, an arc, the end, an arc, a diagonal; each
// diagonal runs into the earlier chamfer's face, and beyond it its vertical
// face goes on tangentially into a corner's cylinder.
//
// A chamfer of 2 on one diagonal takes the whole chain. Its analytic pieces
// reach where the bevel's lower contact leaves the diagonal's face; past
// there it is walked on over the cylinder up to the earlier chamfer's face.
// At the chain's end that walk was right; at its start, before the spine's
// start, the walk's guide was the first edge trimmed backwards and doubled
// back on itself: the walk stalled at once, its piece's first sections were
// never set, and the build failed (TopoDS_Builder::Add, the DS reading the
// solid as a vertex). With the port's patch 1120 the guide there is the
// spine's straight extension: both ends alike, the result valid and closed.
// The facade's chamfers of the same (the model's chains) and of the
// diagonal with the edges beyond it on the earlier chamfer's face, where two
// chamfers meet in a corner at the diagonal's end: the patch keeps the
// guide there as it was, which a guide on past every end broke.

#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Curve.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

constexpr double kTop = 52.0; // the plate's top
constexpr double kD   = 2.0;  // both chamfers
constexpr double kPi  = 3.14159265358979323846;

GProp_GProps volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props;
}

// The straight edge of the body from a to b (either way round).
TopoDS_Edge edge_between(const TopoDS_Shape& body, const gp_Pnt& a, const gp_Pnt& b) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    double f = 0., l = 0.;
    const auto c = BRep_Tool::Curve(e, f, l);
    if (c.IsNull()) {
      continue;
    }
    const gp_Pnt p = c->Value(f), q = c->Value(l);
    if ((p.Distance(a) < 1e-6 && q.Distance(b) < 1e-6) || (p.Distance(b) < 1e-6 && q.Distance(a) < 1e-6)) {
      return e;
    }
  }
  return TopoDS_Edge();
}

// The plate with its top long edges chamfered.
TopoDS_Shape plate() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, 40), 52.0, 40.0, 12.0).Shape();
  BRepFilletAPI_MakeChamfer chamfer(box);
  for (const double y : {-20.0, 20.0}) {
    const TopoDS_Edge e = edge_between(box, gp_Pnt(0, y, kTop), gp_Pnt(52, y, kTop));
    CHECK(!e.IsNull());
    chamfer.Add(kD, e);
  }
  chamfer.Build();
  CHECK(chamfer.IsDone());
  return chamfer.Shape();
}

// The rounded octagon about (32, 0) at height z: sides on x = 12 and 52 and
// y = -20 and 20, diagonals between them, corners of radius 4.
TopoDS_Face rounded_octagon(const double z) {
  // Each corner's centre, counter-clockwise from the one at (+x, +y).
  const double c[8][2] = {{48.0, 6.627416997969522},    {38.62741699796952, 16.0},
                          {25.37258300203048, 16.0},    {16.0, 6.627416997969522},
                          {16.0, -6.627416997969522},   {25.37258300203048, -16.0},
                          {38.62741699796952, -16.0},   {48.0, -6.627416997969522}};
  const double a0[8] = {0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0};
  BRepBuilderAPI_MakeWire wire;
  gp_Pnt previous;
  gp_Pnt first;
  for (int i = 0; i < 8; ++i) {
    const double t0 = a0[i] * kPi / 180.0, t1 = (a0[i] + 45.0) * kPi / 180.0;
    const gp_Pnt p0(c[i][0] + 4.0 * std::cos(t0), c[i][1] + 4.0 * std::sin(t0), z);
    const gp_Pnt pm(c[i][0] + 4.0 * std::cos(0.5 * (t0 + t1)), c[i][1] + 4.0 * std::sin(0.5 * (t0 + t1)), z);
    const gp_Pnt p1(c[i][0] + 4.0 * std::cos(t1), c[i][1] + 4.0 * std::sin(t1), z);
    if (i > 0) {
      wire.Add(BRepBuilderAPI_MakeEdge(previous, p0).Edge());
    } else {
      first = p0;
    }
    wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p0, pm, p1).Value()).Edge());
    previous = p1;
  }
  wire.Add(BRepBuilderAPI_MakeEdge(previous, first).Edge());
  return BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
}

// The plate cut round at its far end: a rectangle beyond x = 32 less the
// rounded octagon, through the plate.
TopoDS_Shape cut_plate() {
  BRepBuilderAPI_MakePolygon rectangle(gp_Pnt(32, -22, 39), gp_Pnt(55, -22, 39), gp_Pnt(55, 22, 39),
                                       gp_Pnt(32, 22, 39), true);
  const TopoDS_Shape box =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(rectangle.Wire(), true).Face(), gp_Vec(0, 0, 14)).Shape();
  const TopoDS_Shape octagon = BRepPrimAPI_MakePrism(rounded_octagon(39.0), gp_Vec(0, 0, 14)).Shape();
  const TopoDS_Shape tool = BRepAlgoAPI_Cut(box, octagon).Shape();
  return BRepAlgoAPI_Cut(plate(), tool).Shape();
}

void test_chamfer_of_a_chain_ending_against_a_chamfer() {
  const TopoDS_Shape body = cut_plate();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // The diagonal on the top face from the corner (+x, +y) to the earlier
  // chamfer's face (y = 18).
  const double s = 2.0 * std::sqrt(2.0); // the corner's arc ends 4 sin 45 off
  const gp_Pnt start(48.0 + s, 6.627416997969522 + s, kTop);
  TopoDS_Edge  diagonal;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More() && diagonal.IsNull(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    double f = 0., l = 0.;
    const auto c = BRep_Tool::Curve(e, f, l);
    if (c.IsNull()) {
      continue;
    }
    const gp_Pnt p = c->Value(f), q = c->Value(l);
    const bool   fromStart = p.Distance(start) < 1e-6 || q.Distance(start) < 1e-6;
    const gp_Pnt other     = p.Distance(start) < 1e-6 ? q : p;
    if (fromStart && std::abs(other.Z() - kTop) < 1e-6 && std::abs(other.Y() - 18.0) < 1e-6) {
      diagonal = e;
    }
  }
  CHECK(!diagonal.IsNull());
  if (diagonal.IsNull()) {
    return;
  }
  BRepFilletAPI_MakeChamfer chamfer(body);
  chamfer.Add(kD, diagonal);
  try {
    OCC_CATCH_SIGNALS
    chamfer.Build();
  } catch (const Standard_Failure& error) {
    std::printf("the chamfer: %s\n", error.what());
  }
  CHECK(chamfer.IsDone());
  if (!chamfer.IsDone()) {
    return;
  }
  const TopoDS_Shape result = chamfer.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  int solids = 0, shells = 0;
  for (TopExp_Explorer ex(result, TopAbs_SOLID); ex.More(); ex.Next()) {
    ++solids;
  }
  for (TopExp_Explorer ex(result, TopAbs_SHELL); ex.More(); ex.Next()) {
    ++shells;
    CHECK(BRep_Tool::IsClosed(ex.Current()));
  }
  CHECK(solids == 1);
  CHECK(shells == 1);
  const GProp_GProps before = volume_of(body), after = volume_of(result);
  const double       removed = before.Mass() - after.Mass();
  std::printf("the chamfer: %d edges in its chain, removed %.9f mm3, centre y %.3e\n", chamfer.NbEdges(1), removed,
              after.CentreOfMass().Y());
  // The chain: the two diagonals, the two arcs and the end.
  CHECK(chamfer.NbContours() == 1);
  CHECK(chamfer.NbEdges(1) == 5);
  // The plate is symmetric about y = 0, and so is the chamfer: its two ends
  // alike (the start's piece is no longer a stall of a hundredth of a
  // millimetre).
  CHECK(std::abs(after.CentreOfMass().Y()) < 1e-6);
  // Along the chain the bevel's prism, 2 mm2 a millimetre over its length on
  // the top face, and somewhat more where it runs on past the diagonals'
  // ends to the earlier chamfer's face.
  const double chain = 2.0 * (12.083 + kPi) + 13.255;
  CHECK(removed > 0.5 * kD * kD * chain);
  CHECK(removed < 0.5 * kD * kD * (chain + 2.0 * 3.5));
  // (As built on Linux: 91.888253 mm3.)
  CHECK(std::abs(removed - 91.888253) < 1e-3);
}

// The same through the geometry facade, as the model chamfers (its own
// tangent chains, the chains stopped at turns): the diagonal alone, and the
// diagonal with the two edges beyond it on the earlier chamfer's face (its
// own edge there and the corner's), which meet the top chain at the
// diagonal's end: a corner of two chamfers there, which built before the
// patch as well and builds as before (the guide there is left as it was).
void test_facade_chamfers_ending_against_a_chamfer() {
  using mitcad::geometry::ChamferSet;
  using mitcad::geometry::Shape;
  const TopoDS_Shape occt = cut_plate();
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(occt, TopAbs_FACE, faces);
  std::vector<Shape::NamedFace> named;
  for (int i = 1; i <= faces.Extent(); ++i) {
    named.push_back({faces(i), {"F9:f" + std::to_string(i)}});
  }
  const Shape body(occt, named);
  const auto name_between = [&](const gp_Pnt& a, const gp_Pnt& b) {
    for (TopExp_Explorer ex(occt, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
      const gp_Pnt p = BRep_Tool::Pnt(TopExp::FirstVertex(e)), q = BRep_Tool::Pnt(TopExp::LastVertex(e));
      if ((p.Distance(a) < 1e-6 && q.Distance(b) < 1e-6) || (p.Distance(b) < 1e-6 && q.Distance(a) < 1e-6)) {
        return body.edge_name(body.edge_index(e));
      }
    }
    return std::string();
  };
  const double s = 2.0 * std::sqrt(2.0);
  // The diagonal's ends on the top face, its end on the earlier chamfer's
  // face, and the corner's end there.
  const gp_Pnt a(48.0 + s, 6.627416997969522 + s, kTop), v(48.0 + s - (18.0 - 6.627416997969522 - s), 18.0, kTop);
  const gp_Pnt k(38.62741699796952 + s, 16.0 + s, kTop - (16.0 + s - 18.0)), w(38.62741699796952, 20.0, kTop - kD);
  const std::string diagonal = name_between(a, v), beyond = name_between(v, k), corner = name_between(k, w);
  CHECK(!diagonal.empty());
  CHECK(!beyond.empty());
  CHECK(!corner.empty());
  ChamferSet set;
  set.spec.distance = kD;
  set.edges = {diagonal};
  double removed = 0.;
  try {
    const auto one = mitcad::geometry::chamfer("F10", body, {set});
    CHECK(BRepCheck_Analyzer(one->occt()).IsValid());
    removed = volume_of(occt).Mass() - volume_of(one->occt()).Mass();
  } catch (const std::exception& error) {
    std::printf("the diagonal: %s\n", error.what());
    CHECK(false);
  }
  std::printf("the diagonal through the facade: removed %.9f mm3\n", removed);
  CHECK(std::abs(removed - 91.888253) < 1e-3);
  set.edges = {diagonal, beyond, corner};
  try {
    const auto three = mitcad::geometry::chamfer("F10", body, {set});
    CHECK(BRepCheck_Analyzer(three->occt()).IsValid());
    int solids = 0;
    for (TopExp_Explorer ex(three->occt(), TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    CHECK(solids == 1);
  } catch (const std::exception& error) {
    std::printf("the diagonal and the edges beyond it: %s\n", error.what());
    CHECK(false);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_chamfer_of_a_chain_ending_against_a_chamfer",
                test::test_chamfer_of_a_chain_ending_against_a_chamfer);
  test::guarded("test_facade_chamfers_ending_against_a_chamfer", test::test_facade_chamfers_ending_against_a_chamfer);
  if (test::failures == 0) {
    std::puts("test_chamfer_end_against_chamfer: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
