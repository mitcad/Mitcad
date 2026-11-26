// SPDX-License-Identifier: MIT
// Port patch 2462 (mitcad#2462, GitHub mitcad/Mitcad#645): a removed face
// that every face round it meets tangentially, with the wall no thicker than
// their rounding, is closed by strips along the normal from its edges.
//
// A box 40 x 30 x 20, its top edges rounded r = 2 mm, shelled inside with
// the top open by w = 1 and 1.5 mm, Sharp (the intersection join) and
// Rounded (the arc join). The roundings' offsets (radius r - w) lie under
// the top all along, parallel to it at its edges, and never meet it: the
// top's edges got no image and the thick solid failed (Sharp) or came out
// invalid (Rounded). Expected: one valid solid whose wall ends at each of
// the top's edges in a strip from the edge down to its offset; its volume
// the body's less the cavity: the box (40 - 2w) x (30 - 2w) x (20 - w) less,
// along the top's edges, what the inner roundings (radius rho = r - w about
// the outer ones' axes) leave of the corner rho x r under each edge, and at
// the four corners what two of them leave of rho x rho x r. A point in the
// wall under the top's edge is in the solid, one past the strip and one in
// the inner rounding's quarter are not.

#include <cmath>
#include <cstdio>

#include <BRepBndLib.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 2.0;

// The box with its four top edges rounded.
TopoDS_Shape box_with_rounded_top() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(40, 30, 20).Shape();
  return port::rounded(box, port::edges_where(box, [](const gp_Pnt& p) { return std::abs(p.Z() - 20) < 1e-6; }),
                       kRadius);
}

// The flat face of the shape lying at height z (null if none).
TopoDS_Shape flat_face_at_height(const TopoDS_Shape& shape, double z) {
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(faces.Current(), box, false, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    box.Get(x1, y1, z1, x2, y2, z2);
    if (std::abs(z1 - z) < 1e-6 && std::abs(z2 - z) < 1e-6) {
      return faces.Current();
    }
  }
  return TopoDS_Shape();
}

TopAbs_State classified(const TopoDS_Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid, p, 1e-7);
  return classifier.State();
}

// The cavity of the wall w (w < r), as above.
double cavity(double w) {
  const double rho = kRadius - w;
  const double kPi = std::acos(-1.0);
  const double edge = rho * kRadius - 0.25 * kPi * rho * rho;              // per unit of length
  const double corner = rho * rho * kRadius - 2.0 / 3.0 * rho * rho * rho; // at each corner
  const double lengths = 2 * (40 - 2 * kRadius) + 2 * (30 - 2 * kRadius);
  return (40 - 2 * w) * (30 - 2 * w) * (20 - w) - edge * lengths - 4 * corner;
}

void test_top_open_wall_no_thicker_than_rounding() {
  const TopoDS_Shape shape = box_with_rounded_top();
  CHECK(port::valid(shape));
  const TopoDS_Shape top = flat_face_at_height(shape, 20);
  CHECK(!top.IsNull());
  if (shape.IsNull() || top.IsNull()) {
    return;
  }
  for (const GeomAbs_JoinType join : {GeomAbs_Intersection, GeomAbs_Arc}) {
    const char* what = join == GeomAbs_Arc ? "Rounded" : "Sharp";
    for (const double wall : {1.0, 1.5}) {
      NCollection_List<TopoDS_Shape> open;
      open.Append(top);
      BRepOffsetAPI_MakeThickSolid maker;
      try {
        OCC_CATCH_SIGNALS
        maker.MakeThickSolidByJoin(shape, open, -wall, 1e-6, BRepOffset_Skin, false, false, join, false);
      } catch (const Standard_Failure& failure) {
        std::fprintf(stderr, "%s %g: the thick solid raised '%s'\n", what, wall, failure.what());
        CHECK(false);
        continue;
      }
      CHECK(maker.IsDone());
      if (!maker.IsDone()) {
        std::fprintf(stderr, "%s %g: the thick solid not done: error %d\n", what, wall,
                     static_cast<int>(maker.MakeOffset().Error()));
        continue;
      }
      const TopoDS_Shape result = maker.Shape();
      CHECK(port::valid(result));
      CHECK(port::count_of(result, TopAbs_SOLID) == 1);
      const double v = port::volume_of(result);
      const double expected = port::volume_of(shape) - cavity(wall);
      if (std::abs(v - expected) > 1e-6 * expected) {
        std::fprintf(stderr, "%s %g: %.4f mm3, expected %.4f\n", what, wall, v, expected);
      }
      CHECK(std::abs(v - expected) <= 1e-6 * expected);
      const double rho = kRadius - wall;
      // In the wall just inside the strip under the top's edge at x = r.
      CHECK(classified(result, gp_Pnt(kRadius - 0.25, 15, 20 - 0.1)) == TopAbs_IN);
      // Past the strip, over the cavity; in the inner rounding's quarter.
      CHECK(classified(result, gp_Pnt(kRadius + 0.25, 15, 20 - 0.1)) == TopAbs_OUT);
      CHECK(classified(result, gp_Pnt(kRadius - 0.5 * rho, 15, 20 - kRadius + 0.5 * rho)) == TopAbs_OUT);
    }
  }
}

} // namespace

MITCAD_PORT_TEST("2462", test_top_open_wall_no_thicker_than_rounding);
