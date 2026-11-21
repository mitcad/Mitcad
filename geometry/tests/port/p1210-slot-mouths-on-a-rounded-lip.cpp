// SPDX-License-Identifier: MIT
// Port patch 1210: a rounding over the mouths of slots whose rounded ends
// touch the top face along a crest line (BRepFilletAPI_MakeFillet::
// BuildOverSlotMouths). Four edges meet at each mouth: the two arcs of the
// rim (tangent there) and the curves where the fin's top C meets the two
// wall pieces. OCCT's corner found no edge between the top and C round the
// vertex and the fillet failed; with the patch the fillet is built as a
// construction of its own (the body rounded with slabs through the mouths
// cut out, the slabs put back with the blends prolonged across them and the
// fins' tops grown up to the blends).
//
// A lip: a disk of radius 93.5 from z = 300.33 to the top z = 307.5, its
// lower outer edge rounded with 5 (the wall above it 2.17 high), and eight
// fins 1.5 thick standing out 1.5 from the wall round it, from z = 301.5
// up, each one's top a cylinder C of radius 0.75 whose crest lies on the
// top's height, touching the rim at one point. The rim (eight arcs between
// the fins) rounded with 2.5: built, valid, one solid, taking off at least
// the rounding's section round the whole rim and at most 5 % more (the
// blends prolonged across the fins' tops). The boundary: the rim rounded
// with 0.5 and 0.7, smaller than C's radius: not a mouth, OCCT's own
// corners build it, valid.

#include <cmath>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRim     = 93.5;   // the lip's radius
constexpr double kTop     = 307.5;  // its top
constexpr double kBottom  = 300.33; // its underside
constexpr double kUnder   = 5.0;    // the rounding of its lower outer edge
constexpr double kRho     = 0.75;   // C's radius, the fins' half thickness
constexpr double kOut     = 1.5;    // how far a fin stands out of the wall
constexpr double kFinFoot = 301.5;  // the fins' bottom
constexpr int    kFins    = 8;

// The lip with its fins.
TopoDS_Shape lip_with_fins() {
  const TopoDS_Shape disk =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kBottom), gp_Dir(0, 0, 1)), kRim, kTop - kBottom).Shape();
  const std::vector<TopoDS_Edge> foot = port::edges_where(disk, [](const gp_Pnt& m) {
    return std::abs(m.Z() - kBottom) < 1e-7 && std::hypot(m.X(), m.Y()) > kRim - 1e-6;
  });
  TopoDS_Shape body = port::rounded(disk, foot, kUnder);
  CHECK(port::valid(body));
  // A fin along x from inside the wall, its back cut by the wall's cylinder
  // (so that its crest meets the top at the rim only).
  const double            axis = kTop - kRho, x0 = kRim - 3.0;
  BRepBuilderAPI_MakeWire profile;
  profile.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, -kRho, kFinFoot), gp_Pnt(x0, kRho, kFinFoot)).Edge());
  profile.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, kRho, kFinFoot), gp_Pnt(x0, kRho, axis)).Edge());
  profile.Add(BRepBuilderAPI_MakeEdge(
                GC_MakeArcOfCircle(gp_Pnt(x0, kRho, axis), gp_Pnt(x0, 0, kTop), gp_Pnt(x0, -kRho, axis)).Value())
                .Edge());
  profile.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, -kRho, axis), gp_Pnt(x0, -kRho, kFinFoot)).Edge());
  const TopoDS_Shape prism =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(profile.Wire(), true).Face(), gp_Vec(3.0 + kOut, 0, 0)).Shape();
  const TopoDS_Shape wall = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 280), gp_Dir(0, 0, 1)), kRim, 40).Shape();
  const TopoDS_Shape fin  = BRepAlgoAPI_Cut(prism, wall).Shape();
  for (int k = 0; k < kFins; ++k) {
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 2.0 * test::kPi * k / kFins);
    body = BRepAlgoAPI_Fuse(body, BRepBuilderAPI_Transform(fin, turn, true).Shape()).Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(body, true, true, true);
  unify.Build();
  return unify.Shape();
}

// The rim's arcs rounded with r; a null shape where the fillet fails.
TopoDS_Shape rim_rounded(const TopoDS_Shape& body, double r) {
  const std::vector<TopoDS_Edge> rim = port::edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.Z() - kTop) < 1e-6 && std::abs(std::hypot(m.X(), m.Y()) - kRim) < 1e-6;
  });
  CHECK(static_cast<int>(rim.size()) == kFins);
  return port::rounded(body, rim, r);
}

// The rim rounded with 2.5 over the eight mouths.
void test_rounding_over_slot_mouths_on_a_rounded_lip() {
  const TopoDS_Shape body = lip_with_fins();
  CHECK(port::valid(body));
  const TopoDS_Shape result = rim_rounded(body, 2.5);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  // The rounding's section (its centroid 0.2234 r from the corner) round
  // the whole rim.
  const double r       = 2.5;
  const double ring    = test::fillet_loss(r) * 2.0 * test::kPi * (kRim - (10.0 - 3.0 * test::kPi) / (3.0 * (4.0 - test::kPi)) * r);
  const double removed = port::volume_of(body) - port::volume_of(result);
  CHECK(removed > ring);
  CHECK(removed < 1.05 * ring);
}

// The boundary: rounded with 0.5 and 0.7, smaller than C's radius: not a
// mouth, built by OCCT's own corners.
void test_rounding_smaller_than_the_fins_tops_is_as_before() {
  const TopoDS_Shape body = lip_with_fins();
  for (const double r : {0.5, 0.7}) {
    const TopoDS_Shape result = rim_rounded(body, r);
    CHECK(port::valid(result));
  }
}

} // namespace

MITCAD_PORT_TEST("1210", test_rounding_over_slot_mouths_on_a_rounded_lip);
MITCAD_PORT_TEST("1210", test_rounding_smaller_than_the_fins_tops_is_as_before);
