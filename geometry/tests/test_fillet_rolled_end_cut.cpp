// SPDX-License-Identifier: MIT
// A rounding rolling on an edge whose ends lie past the faces at the ends of
// its edge, built with OCCT's fillet alone. A block's slot ends at a wall;
// both long edges of the slot's ceiling are rounded with 9 (concave), and a
// groove of radius 5 runs under the ceiling across the wall's foot, cutting
// a notch into the wall and leaving the wall's face a strip only 0.86 high
// above the groove. The groove's rim on the strip is rounded with 10: the
// ball's contact on the strip lies above the strip's top, so the ball rolls
// on the strip's top edges (the ends of the 9 roundings and of the ceiling)
// and consumes the strip. At both ends of the rim the notch's walls cut the
// rounding, and past each end the groove's contact runs off into the wall
// beside the notch. OCCT ended the walk past the ends on no edge and failed
// at both corners. The blend is now walked again with the groove's surface
// unbounded up to where its ball comes tangent to the 9 rounding, run on
// along its tangents, and cut there by the wall beside the notch, the
// notch's wall and the 9 rounding, which grow up to it. A stored design
// whose groove runs under a slot's end the same way is the case behind the
// patch 0990 of the OCCT port (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <Bnd_Box.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kSlot  = 9.0;  // the slot's ceiling roundings
constexpr double kRim   = 10.0; // the groove's rim on the strip
constexpr double kWall  = 6.0;  // the slot's end wall, x = kWall
constexpr double kGroove = 5.0; // the groove's radius
constexpr double kAxisX = 8.8;  // its axis, along y, at z = kAxisZ
constexpr double kAxisZ = -5.0;
constexpr double kNotch0 = -34.0, kNotch1 = -27.0; // the notch in the wall

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The block with the slot (y in [-40, -21], under the ceiling z = 0, from
// the wall x = kWall on), its ceiling's long edges rounded with kSlot, the
// groove cut: west of the wall's 1 mm thickness (x < kWall - 1) along its
// whole length, and through the wall within the notch.
TopoDS_Shape grooved_slot() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-6, -50, -25), gp_Pnt(20, 0, 5)).Shape();
  const TopoDS_Shape slot  = BRepPrimAPI_MakeBox(gp_Pnt(kWall, -40, -26), gp_Pnt(21, -21, 0)).Shape();
  const TopoDS_Shape cut   = BRepAlgoAPI_Cut(block, slot).Shape();
  BRepFilletAPI_MakeFillet slotRounds(cut);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(cut, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
    const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
    const bool ceiling = std::abs(a.Z()) < 1e-9 && std::abs(b.Z()) < 1e-9;
    const bool side = (std::abs(a.Y() + 21) < 1e-9 && std::abs(b.Y() + 21) < 1e-9)
                      || (std::abs(a.Y() + 40) < 1e-9 && std::abs(b.Y() + 40) < 1e-9);
    if (ceiling && side) {
      slotRounds.Add(kSlot, TopoDS::Edge(edges(i)));
    }
  }
  slotRounds.Build();
  CHECK(slotRounds.IsDone());
  if (!slotRounds.IsDone()) {
    return TopoDS_Shape();
  }
  // (The groove: west of the wall's 1 mm thickness, and through it at the notch.)
  const gp_Ax2 axis(gp_Pnt(kAxisX, -60, kAxisZ), gp_Dir(0, 1, 0));
  const TopoDS_Shape full   = BRepPrimAPI_MakeCylinder(axis, kGroove, 70).Shape();
  const TopoDS_Shape before = BRepPrimAPI_MakeBox(gp_Pnt(kWall - 1, -70, -30), gp_Pnt(30, kNotch0, 10)).Shape();
  const TopoDS_Shape after  = BRepPrimAPI_MakeBox(gp_Pnt(kWall - 1, kNotch1, -30), gp_Pnt(30, 20, 10)).Shape();
  const TopoDS_Shape groove = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(full, before).Shape(), after).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(slotRounds.Shape(), groove).Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

void test_rim_rolled_over_a_strip_cut_by_the_notch_walls() {
  const TopoDS_Shape body = grooved_slot();
  if (body.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // The groove's rim on the strip: the line x = kWall at the groove's height
  // there, across the notch.
  const double zRim = kAxisZ + std::sqrt(kGroove * kGroove - (kAxisX - kWall) * (kAxisX - kWall));
  BRepFilletAPI_MakeFillet fillet(body);
  int added = 0;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
    const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
    if (std::abs(a.X() - kWall) < 1e-7 && std::abs(b.X() - kWall) < 1e-7 && std::abs(a.Z() - zRim) < 1e-7
        && std::abs(b.Z() - zRim) < 1e-7) {
      fillet.Add(kRim, TopoDS::Edge(edges(i)));
      ++added;
    }
  }
  CHECK(added == 1);
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
  }
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    std::printf("the rim's rounding: not done (%d faulty vertices)\n", fillet.NbFaultyVertices());
    return;
  }
  const TopoDS_Shape rounded = fillet.Shape();
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  const double loss = volume_of(body) - volume_of(rounded);
  std::printf("the rim's rounding: material taken %.7f\n", loss);
  // (At most the material between the strip, the groove and the ceiling
  // over the notch.)
  CHECK(loss > 0.0 && loss < (kNotch1 - kNotch0) * (kWall - (kAxisX - kGroove)) * (0.0 - (kAxisZ - kGroove)));
  // The notch's walls grow up to the cut, above the groove's rim, and each
  // end of the 9 rounding over the notch grows a little past the slot's
  // wall, up to the cut by the notch's wall: the faces at the ends cut the
  // blend.
  double wallTop[2] = {-1e9, -1e9};
  double slotWest   = 1e9;
  int    faces = 0, small = 0;
  for (TopExp_Explorer ex(rounded, TopAbs_FACE); ex.More(); ex.Next(), ++faces) {
    const TopoDS_Face& f = TopoDS::Face(ex.Current());
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f, props);
    if (std::abs(props.Mass()) < 1e-9) {
      ++small;
    }
    Bnd_Box box;
    BRepBndLib::Add(f, box);
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    const BRepAdaptor_Surface s(f);
    if (s.GetType() == GeomAbs_Plane && x0 > kWall - 1 - 1e-2 && x1 < kWall + 1e-2 && y1 - y0 < 1e-2) {
      const int k = std::abs(y0 - kNotch0) < 1e-2 ? 0 : (std::abs(y0 - kNotch1) < 1e-2 ? 1 : -1);
      if (k >= 0) {
        wallTop[k] = std::max(wallTop[k], z1);
      }
    }
    if (s.GetType() == GeomAbs_Cylinder && std::abs(s.Cylinder().Radius() - kSlot) < 1e-7) {
      slotWest = std::min(slotWest, x0);
    }
  }
  std::printf("the rim's rounding: %d faces, notch walls up to z %.5f and %.5f, slot roundings from x %.6f\n",
              faces, wallTop[0], wallTop[1], slotWest);
  CHECK(small == 0);
  CHECK(wallTop[0] > zRim + 0.1 && wallTop[1] > zRim + 0.1);
  CHECK(slotWest < kWall - 1e-4);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_rolled_end_cut) {
  test::guarded("test_rim_rolled_over_a_strip_cut_by_the_notch_walls",
                test::test_rim_rolled_over_a_strip_cut_by_the_notch_walls);
  if (test::failures == 0) {
    std::puts("test_fillet_rolled_end_cut: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
