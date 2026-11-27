// SPDX-License-Identifier: MIT
// OCCT's face merge after booleans (ShapeUpgrade_UnifySameDomain, as
// BRepAlgoAPI's SimplifyResult calls it) on faces of one cylinder whose
// surfaces are parametrised the other way round, with the port's patch 1040
// (mitcad#642): a quarter of a cylinder (axis up) and a sector before or
// after it mirrored top to bottom (axis down, its frame indirect), fused.
// Without the patch the other face's pcurves were turned onto the reference
// face's surface (the first face's) by the unsigned angle between the X
// directions, the wrong way when the other's lies behind the reference's:
// the two faces stayed apart (and in a bin's corner, cut by an extrusion
// going down, the merged body was invalid).

#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace {

using test::kPi;

constexpr double kRadius = 2.75;
constexpr double kHeight = 1.0;

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

int faces(const TopoDS_Shape& shape, bool cylindrical) {
  int n = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const bool on_cylinder = BRepAdaptor_Surface(TopoDS::Face(ex.Current())).GetType() == GeomAbs_Cylinder;
    n += !cylindrical || on_cylinder ? 1 : 0;
  }
  return n;
}

// The merge as BRepAlgoAPI_BuilderAlgo::SimplifyResult runs it after a
// boolean (the fuzzy value its default, the input kept).
TopoDS_Shape merged(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, true);
  unify.SetLinearTolerance(Precision::Confusion());
  unify.SetAngularTolerance(Precision::Angular());
  unify.SetSafeInputMode(true);
  unify.AllowInternalEdges(false);
  unify.Build();
  return unify.Shape();
}

// A quarter of the cylinder about the z axis (its X direction along +y),
// and the sector of `angle` before it (`behind`) or after it, mirrored
// top to bottom, fused with the quarter first or the sector first.
void check_merge(double angle, bool behind, bool quarter_first) {
  BRepPrimAPI_MakeCylinder quarter(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), kRadius, kHeight,
                                   kPi / 2);
  gp_Dir x(0, 1, 0);
  x.Rotate(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), behind ? -angle : kPi / 2);
  BRepPrimAPI_MakeCylinder sector(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), x), kRadius, kHeight, angle);
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp_Pnt(0, 0, kHeight / 2), gp_Dir(0, 0, 1)));
  const TopoDS_Shape mirrored = BRepBuilderAPI_Transform(sector.Shape(), mirror, true).Shape();
  BRepAlgoAPI_Fuse fuse(quarter_first ? quarter.Shape() : mirrored, quarter_first ? mirrored : quarter.Shape());
  CHECK(fuse.IsDone() && !fuse.HasErrors());
  const TopoDS_Shape fused = fuse.Shape();
  CHECK(faces(fused, true) == 2);
  const TopoDS_Shape result = merged(fused);
  if (faces(result, true) != 1 || faces(result, false) != 5) {
    std::fprintf(stderr, "angle %g %s, %s first: %d faces, %d of them cylindrical\n", angle,
                 behind ? "behind" : "ahead", quarter_first ? "quarter" : "sector", faces(result, false),
                 faces(result, true));
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // One cylindrical face, the two planar sides and the top and bottom.
  CHECK(faces(result, true) == 1);
  CHECK(faces(result, false) == 5);
  CHECK_NEAR_TOL(volume(result), volume(fused), 1e-9);
}

void test_sector_behind_with_its_axis_reversed() {
  // The sector's X direction 0.5 and 1.2 rad behind the quarter's: the
  // angle from the quarter's to it is negative, and the unsigned one turned
  // its pcurves to the other side of the quarter.
  for (const double angle : {0.5, 1.2}) {
    for (const bool quarter_first : {true, false}) {
      check_merge(angle, true, quarter_first);
    }
  }
}

void test_sector_ahead_with_its_axis_reversed() {
  // Its X direction a quarter turn ahead: with the quarter first the angle
  // from the quarter's to it is positive (merged before the patch as well);
  // with the sector first the merge takes the sector's surface for the
  // reference, and the quarter's X direction lies behind the sector's.
  for (const bool quarter_first : {true, false}) {
    check_merge(0.5, false, quarter_first);
  }
}

} // namespace

MITCAD_BUNDLED_TEST(merge_reversed_axis) {
  test::guarded("sector_behind_with_its_axis_reversed", test_sector_behind_with_its_axis_reversed);
  test::guarded("sector_ahead_with_its_axis_reversed", test_sector_ahead_with_its_axis_reversed);
  if (test::failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("merge of faces with reversed axes: ok\n");
  return 0;
}
