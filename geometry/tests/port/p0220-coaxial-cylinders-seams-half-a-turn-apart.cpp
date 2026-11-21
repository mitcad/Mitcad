// SPDX-License-Identifier: MIT
// Port patch 0220: the face merge (ShapeUpgrade_UnifySameDomain) merges two
// faces of one cylinder whose seams lie half a period apart.
//
// A cylinder of radius 10 from z = 0 to 10 with its seam at +x, and on it a
// cylinder (or a revolved ring) of the same radius and axis from z = 10 to
// 12 with its seam at -x, fused with BRepAlgoAPI_Fuse: the result has two
// faces on the one cylindrical surface, and the circle between them at
// z = 10 is cut at both seams' ends, at u = 0 (V1) and u = pi (V2), into two
// half circles. The face merge removes both seams and both half circles and
// builds the merged face's seam again from them (ReconstructMissedSeam):
// from the bottom circle up the first seam to V1, then along a half circle
// to V2, then up the second seam. At V1 a removed edge that ENDS there (in
// the orientation it was collected with) was judged by its far end, V2:
// taken (turned round) where V2 lay more than half a period from the
// current point, dropped where it lay less. V2's u is pi (or pi plus a
// period) and the current point's u is 0 or 2 pi: the far end lies exactly
// half a period away (|pi - 0| = pi = Uperiod / 2, computed exactly, the
// upper body's seam point being (-10, 0, z) to the last bit), so neither
// `aUdiff > Uperiod / 2` nor `aUdiff < Uperiod / 2` holds and the half
// circle is taken by neither test; where both half circles end at V1, no
// edge goes on from there, the wire cannot be built and the two faces stay
// apart (the check below: two faces on the cylinder of radius 10 instead
// of one). With the patch such an edge is turned to start at V1 first and
// its own end there is compared with the current point (difference 0 or a
// period, then its curves swapped), so it is taken.
//
// Which half circles end at V1 depends on the orientations the removed
// edges keep from the faces they were collected from, so the test sweeps
// the cases: the lower cylinder made upwards or downwards (its u runs the
// other way round), the upper one a cylinder upwards or downwards or a ring
// revolved about +z or -z (the case of mitcad#284: a revolution joined onto
// an extruded cylinder), and both orders of the fuse's arguments. Without
// the patch at least one of these sixteen keeps two faces; with it every
// one merges into one face, the result valid and of the bodies' volume.
//
// The boundary: the same bodies with both seams at +x. The circle between
// the faces is then one closed edge from V1 to V1, the seams continue each
// other, and no half circle's far end is compared: every case merges with
// and without the patch (the stock merge of stacked cylinders).

#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 10.0; // both bodies' outer radius
constexpr double kLow    = 10.0; // the lower cylinder's height (z = 0 to 10)
constexpr double kHigh   = 2.0;  // the upper body's height (z = 10 to 12)
constexpr double kInner  = 8.0;  // the ring's inner radius

// The lower cylinder, z = 0 to 10, its seam at +x; made from z = 10
// downwards when `down` (its u runs the other way round).
TopoDS_Shape lower(bool down) {
  const gp_Ax2 axes = down ? gp_Ax2(gp_Pnt(0, 0, kLow), -gp::DZ(), gp::DX()) : gp_Ax2(gp::Origin(), gp::DZ(), gp::DX());
  return BRepPrimAPI_MakeCylinder(axes, kRadius, kLow).Shape();
}

// The upper body, z = 10 to 12, its seam at -x when `opposite` (else at
// +x): kind 0 a cylinder made upwards, 1 one made downwards, 2 a ring
// revolved about +z, 3 one revolved about -z.
TopoDS_Shape upper(int kind, bool opposite) {
  const double side = opposite ? -1.0 : 1.0;
  const gp_Dir seam(side, 0, 0);
  if (kind == 0) {
    return BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kLow), gp::DZ(), seam), kRadius, kHigh).Shape();
  }
  if (kind == 1) {
    return BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kLow + kHigh), -gp::DZ(), seam), kRadius, kHigh).Shape();
  }
  // The ring's section in the plane y = 0 on the seam's side, revolved a
  // whole turn: the revolution's seam is the section itself.
  BRepBuilderAPI_MakePolygon section(gp_Pnt(side * kInner, 0, kLow), gp_Pnt(side * kRadius, 0, kLow),
                                     gp_Pnt(side * kRadius, 0, kLow + kHigh), gp_Pnt(side * kInner, 0, kLow + kHigh),
                                     true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(section.Wire(), true).Face();
  const gp_Ax1      axis(gp::Origin(), kind == 2 ? gp::DZ() : -gp::DZ());
  return BRepPrimAPI_MakeRevol(face, axis, 2.0 * test::kPi).Shape();
}

// The faces of `shape` on the cylinder of radius 10 about the z axis (by
// a point inside each face that is not planar).
int outer_wall_faces(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face&        face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() == GeomAbs_Plane) {
      continue;
    }
    double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const gp_Pnt p = surface.Value(0.5 * (u0 + u1), 0.5 * (v0 + v1));
    if (std::abs(std::hypot(p.X(), p.Y()) - kRadius) < 1e-6) {
      ++count;
    }
  }
  return count;
}

// The fused bodies after the face merge; null when either throws.
TopoDS_Shape merged(const TopoDS_Shape& first, const TopoDS_Shape& second) {
  try {
    OCC_CATCH_SIGNALS
    BRepAlgoAPI_Fuse fuse(first, second);
    if (!fuse.IsDone()) {
      return TopoDS_Shape();
    }
    ShapeUpgrade_UnifySameDomain unify(fuse.Shape(), true, true, false);
    unify.Build();
    return unify.Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fuse and merge: %s\n", failure.what());
    return TopoDS_Shape();
  }
}

// Every case with the seams `opposite` (or both at +x): the number of
// results that are not one valid solid of the bodies' volume with one face
// on the cylinder of radius 10.
int unmerged_cases(bool opposite) {
  int bad = 0;
  for (const bool down : {false, true}) {
    for (int kind = 0; kind < 4; ++kind) {
      for (const bool swapped : {false, true}) {
        const TopoDS_Shape low    = lower(down);
        const TopoDS_Shape high   = upper(kind, opposite);
        const TopoDS_Shape result = swapped ? merged(high, low) : merged(low, high);
        const double       area   = kind < 2 ? kRadius * kRadius : kRadius * kRadius - kInner * kInner;
        const double       volume = test::kPi * (kRadius * kRadius * kLow + area * kHigh);
        const int          walls  = result.IsNull() ? 0 : outer_wall_faces(result);
        const bool         ok     = port::valid(result) && port::count_of(result, TopAbs_SOLID) == 1 && walls == 1
                       && test::near(port::volume_of(result), volume, 1e-7);
        if (!ok) {
          std::printf("  lower made %s, upper kind %d, %s first: %d face(s) on the outer wall%s\n",
                      down ? "downwards" : "upwards", kind, swapped ? "upper" : "lower", walls,
                      port::valid(result) ? "" : ", not valid");
          ++bad;
        }
      }
    }
  }
  std::printf("seams %s: %d of 16 cases not merged into one wall\n", opposite ? "half a turn apart" : "together", bad);
  return bad;
}

void test_seams_half_a_turn_apart_merge() {
  CHECK(unmerged_cases(true) == 0);
}

// The boundary: the seams on one line, merged with and without the patch.
void test_seams_together_merge() {
  CHECK(unmerged_cases(false) == 0);
}

} // namespace

MITCAD_PORT_TEST("0220", test_seams_half_a_turn_apart_merge);
MITCAD_PORT_TEST("0220", test_seams_together_merge);
