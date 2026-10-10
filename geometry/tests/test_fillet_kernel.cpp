// SPDX-License-Identifier: MIT
// Roundings that OCCT's fillet (TKFillet) failed on or broke, built here
// with OCCT alone on synthetic shapes: the cases behind the patches 0020 and
// later of the OCCT port (third_party/vcpkg-ports, mitcad#122).

#include <cmath>
#include <string>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "check.hpp"

namespace test {
namespace {

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// Builds the operation; false when OCCT throws or does not finish.
bool build(BRepFilletAPI_LocalOperation& op) {
  try {
    OCC_CATCH_SIGNALS
    op.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet kernel test: %s\n", failure.what());
    return false;
  }
  return op.IsDone();
}

// True when both ends of the edge lie at height z.
bool at_height(const TopoDS_Edge& edge, double z) {
  return std::abs(BRep_Tool::Pnt(TopExp::FirstVertex(edge)).Z() - z) < 1e-6 &&
         std::abs(BRep_Tool::Pnt(TopExp::LastVertex(edge)).Z() - z) < 1e-6;
}

// A teardrop prism 20 high: two lines from the tip at the origin, tangent
// to a circle of radius 10 about (20, 0), and the circle's far arc.
TopoDS_Shape teardrop_prism() {
  const double s = 10 * std::sqrt(3.0) / 2;
  const gp_Pnt tip(0, 0, 0), upper(15, s, 0), lower(15, -s, 0), far(30, 0, 0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(tip, upper).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(upper, far, lower).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(lower, tip).Edge());
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire()).Face(), gp_Vec(0, 0, 20)).Shape();
}

// The rim of a teardrop prism chamfered 2, then the lower edges of the
// chamfer rounded: one contour of tangent edges closed by its sharp corner
// at the tip, where four sharp edges meet (patch 0021). OCCT matched only
// the contour's first end there and threw
// "TopOpeBRepDS_DataStructure::Point".
void test_contour_closed_at_a_sharp_corner() {
  const TopoDS_Shape prism = teardrop_prism();
  BRepFilletAPI_MakeChamfer chamfer(prism);
  for (TopExp_Explorer e(prism, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (at_height(edge, 20) && chamfer.Contour(edge) == 0) {
      chamfer.Add(2.0, edge);
    }
  }
  CHECK(build(chamfer));
  const TopoDS_Shape chamfered = chamfer.Shape();
  BRepFilletAPI_MakeFillet fillet(chamfered);
  for (TopExp_Explorer e(chamfered, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (at_height(edge, 18) && fillet.Contour(edge) == 0) {
      fillet.Add(1.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  // A rolling ball of radius 1 between faces whose normals are 45 degrees
  // apart takes (tan 22.5 deg - pi/8) r^2 of the section along the 76.5 mm
  // of the contour, a little less at the tip.
  const double length = 2 * std::sqrt(300.0) + 10 * 4 * kPi / 3;
  const double loss = (std::tan(kPi / 8) - kPi / 8) * length;
  const double removed = volume_of(chamfered) - volume_of(fillet.Shape());
  CHECK(removed > 0.98 * loss && removed < loss);
}

// A prism whose top is two planes meeting at 3 degrees along a ridge 20 long,
// the ridge rounded with a radius of 50 (patch 0022). OCCT took the faces
// for tangent (within 0.1 rad) and refused the edge.
void test_ridge_between_faces_at_a_small_angle() {
  const double alpha = 3 * kPi / 180;
  BRepBuilderAPI_MakePolygon outline;
  outline.Add(gp_Pnt(0, 0, 0));
  outline.Add(gp_Pnt(40, 0, 0));
  outline.Add(gp_Pnt(40, 10, 0));
  outline.Add(gp_Pnt(20, 10 + 20 * std::tan(alpha / 2), 0));
  outline.Add(gp_Pnt(0, 10, 0));
  outline.Close();
  const TopoDS_Shape prism =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire()).Face(), gp_Vec(0, 0, 20)).Shape();
  BRepFilletAPI_MakeFillet fillet(prism);
  for (TopExp_Explorer e(prism, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (std::abs(BRep_Tool::Pnt(TopExp::FirstVertex(edge)).X() - 20) < 1e-9 &&
        std::abs(BRep_Tool::Pnt(TopExp::LastVertex(edge)).X() - 20) < 1e-9) {
      fillet.Add(50, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  const double loss = 50 * 50 * (std::tan(alpha / 2) - alpha / 2) * 20;
  CHECK_NEAR_TOL(volume_of(prism) - volume_of(fillet.Shape()), loss, 1e-4);

  // Faces within the angular tolerance stay tangent: nothing to round.
  BRepFilletAPI_MakeFillet flat(prism);
  flat.SetParams(0.06, 1e-4, 1e-5, 1e-4, 1e-5, 1e-3);
  for (TopExp_Explorer e(prism, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (std::abs(BRep_Tool::Pnt(TopExp::FirstVertex(edge)).X() - 20) < 1e-9 &&
        std::abs(BRep_Tool::Pnt(TopExp::LastVertex(edge)).X() - 20) < 1e-9) {
      flat.Add(50, edge);
    }
  }
  CHECK(flat.NbContours() == 0);
}

} // namespace

void fillet_kernel_tests() {
  guarded("test_contour_closed_at_a_sharp_corner", test_contour_closed_at_a_sharp_corner);
  guarded("test_ridge_between_faces_at_a_small_angle", test_ridge_between_faces_at_a_small_angle);
}

} // namespace test
