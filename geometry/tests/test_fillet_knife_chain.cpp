// SPDX-License-Identifier: MIT
// A knife edge running on into an ordinary edge on the same line, rounded
// in one fillet, with OCCT's fillet alone (mitcad#1988, port patch 1790).
//
// A block 130 by 210 by 40 (x from -65 to 65, y from 0 to 210) with a bite
// cut from below 20 deep: the cylinder of radius 65 about the z axis. Along
// the line x = 65, y = 0 the side wall x = 65 meets the bite's cylinder
// tangentially below z = 20 (a knife edge, the wedge between them of 0
// degrees) and the front face y = 0 square above it: one tangent chain of
// two edges. The ball of a rounding of radius r lies sqrt(260 r) from the
// front along the knife edge (where the wedge is 2 r thick) and in the
// corner along the square edge: it jumps at their common vertex on the
// bite's end plane z = 20. The two roundings do not meet there; the body
// asked for would have the end plane grown under the knife's tip up to the
// corner's rounding and a new edge along the wall between their contacts,
// which no corner of OCCT's fillet builds.
//
// OCCT splits the chain at the vertex into two contours; its corner of
// more than three edges then built a valid body of the wrong shape (with a
// plate far from the edges and more volume than the block). Since 1790 the
// fillet refuses that corner in words at that vertex. The knife edge alone
// still rounds, its end on the bite's end plane, the plane grown over the
// tip's section.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kDepth = 20.0;  // the bite's
constexpr double kHeight = 40.0; // the block's

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

TopoDS_Shape bitten_block() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-65, 0, 0), gp_Pnt(65, 210, kHeight)).Shape();
  const TopoDS_Shape bite =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp::DZ(), gp::DX()), 65.0, kDepth + 1.0).Shape();
  return BRepAlgoAPI_Cut(box, bite).Shape();
}

// The edges on the line x = 65, y = 0.
std::vector<TopoDS_Edge> line_edges(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> found;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(edges(i));
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    if (std::abs(a.X() - 65) < 1e-7 && std::abs(b.X() - 65) < 1e-7 && std::abs(a.Y()) < 1e-7 &&
        std::abs(b.Y()) < 1e-7) {
      found.push_back(e);
    }
  }
  return found;
}

// The tip's section the knife edge's rounding takes (as in
// test_fillet_knife_edge.cpp): the wall to the ball's contact, the ball's
// circle round the tip's side, the circle of 65 back to the cusp.
double tip_area(double r) {
  const double cx = 65.0 - r;
  const double cy = std::sqrt((65.0 + r) * (65.0 + r) - cx * cx);
  const double b = std::atan2(-cy, -cx);
  const double a = std::atan2(cy, cx);
  double twice = 65.0 * cy;
  twice += r * r * b + r * cx * std::sin(b) - r * cy * (std::cos(b) - 1.0);
  twice += -65.0 * 65.0 * a;
  return twice / 2.0;
}

void test_knife_into_square_edge() {
  const TopoDS_Shape body = bitten_block();
  CHECK_NEAR(volume_of(body), 130.0 * 210.0 * kHeight - kPi * 65.0 * 65.0 / 2.0 * kDepth);
  const std::vector<TopoDS_Edge> line = line_edges(body);
  CHECK(line.size() == 2);
  for (const double r : {1.0, 3.0}) {
    std::printf(" the knife edge and the square edge on its line, radius %.1f\n", r);
    try {
      OCC_CATCH_SIGNALS
      BRepFilletAPI_MakeFillet fillet(body);
      for (const TopoDS_Edge& e : line) {
        fillet.Add(r, e);
      }
      // Two contours: the chain is split at the vertex.
      CHECK(fillet.NbContours() == 2);
      fillet.Build();
      CHECK(!fillet.IsDone());
      CHECK(fillet.Failure() == ChFi3d_Builder::Failure_Corner);
      CHECK(fillet.NbFaultyVertices() == 1);
      if (fillet.NbFaultyVertices() == 1) {
        const gp_Pnt at = BRep_Tool::Pnt(fillet.FaultyVertex(1));
        CHECK(at.Distance(gp_Pnt(65, 0, kDepth)) < 1e-7);
        const std::string message = fillet.FailedCornerMessage(1).ToCString();
        std::printf("  refused: %s\n", message.c_str());
        CHECK(message.find("knife edge") != std::string::npos);
      }
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "  threw: %s\n", failure.what());
      CHECK(false);
    }
  }
}

// Each edge alone is no such corner: the knife edge's rounding ends on the
// bite's end plane, which grows over the tip's section (its volume the
// block's less the tip's section times 20).
void test_knife_edge_alone() {
  const TopoDS_Shape body = bitten_block();
  const double block_volume = volume_of(body);
  for (const TopoDS_Edge& e : line_edges(body)) {
    if (BRep_Tool::Pnt(TopExp::FirstVertex(e)).Z() > kDepth + 1e-7 ||
        BRep_Tool::Pnt(TopExp::LastVertex(e)).Z() > kDepth + 1e-7) {
      continue;
    }
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(1.0, e);
    fillet.Build();
    CHECK(fillet.IsDone());
    if (fillet.IsDone()) {
      CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
      CHECK(count(fillet.Shape(), TopAbs_SOLID) == 1);
      CHECK_NEAR(volume_of(fillet.Shape()), block_volume - kDepth * tip_area(1.0));
    }
  }
}

// MitCad's fillet of the square edge with its tangent chain (the knife
// edge): the knife edge first, then the square edge on its result
// (knife_line_apart): one valid solid, the block less the tip's section
// and the corner's, each times 20.
void test_fillet_of_the_chain() {
  const TopoDS_Shape block = bitten_block();
  const mitcad::geometry::ShapePtr body = mitcad::geometry::import_body(block, "F1", 0);
  std::string square;
  for (int i = 0; i < body->edge_count(); ++i) {
    const TopoDS_Edge& e = body->edge(i);
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    if (std::abs(a.X() - 65) < 1e-7 && std::abs(b.X() - 65) < 1e-7 && std::abs(a.Y()) < 1e-7 &&
        std::abs(b.Y()) < 1e-7 && std::min(a.Z(), b.Z()) > kDepth - 1e-7) {
      square = body->edge_name(i);
    }
  }
  CHECK(!square.empty());
  for (const double r : {1.0, 3.0}) {
    std::printf(" MitCad's fillet of the chain, radius %.1f\n", r);
    const mitcad::geometry::ShapePtr rounded = mitcad::geometry::fillet("F5", *body, {square}, r);
    CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
    CHECK(count(rounded->occt(), TopAbs_SOLID) == 1);
    CHECK(rounded->notes().empty());
    const double expected =
        volume_of(block) - kDepth * tip_area(r) - (kHeight - kDepth) * r * r * (1.0 - kPi / 4.0);
    const double volume = volume_of(rounded->occt());
    std::printf("  volume %.9f, expected %.9f\n", volume, expected);
    CHECK_NEAR(volume, expected);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_knife_chain) {
  test::guarded("knife_edge_alone", test::test_knife_edge_alone);
  test::guarded("knife_into_square_edge", test::test_knife_into_square_edge);
  test::guarded("fillet_of_the_chain", test::test_fillet_of_the_chain);
  return test::failures == 0 ? 0 : 1;
}
