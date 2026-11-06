// SPDX-License-Identifier: MIT
// A boss's foot and rim rounded together with half the boss's height
// (mitcad#1392), with OCCT's fillet alone, on synthetic bodies: a bar whose
// top is a half round of radius R (one cylinder face with a boss of the same
// radius and axis standing h out of the bar's end face), its foot (the half
// circle below the top) and its rim (the whole circle at the boss's end)
// rounded together. At radius h / 2 both roundings' contacts on the cylinder
// lie on the circle at the boss's middle, the foot's open contact along part
// of the rim's closed one: the foot's rounding rides on that part (the port's
// patch 1340), the cylinder vanishes there and keeps the bar's top up to the
// rim's contact. Without the patch the stripes' check took the common
// contact for a clash and the fillet failed. The cylinder's seam lies on the
// top, at an end of the foot (where the rim's closed contact starts, as on
// the reported design's boss, mitcad#1385) or below the top (the foot's
// contact running over it in two pieces).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>

#include "check.hpp"

namespace test {
namespace {

using EdgeFaces = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

constexpr double kR   = 4.5;  // the bar's top and the boss
constexpr double kBar = 10.;  // the bar's length

// The bar (x from 0 to kBar, its top a half round of radius kR about the x
// axis, below it a box down to z = -10) with the boss from x = -h to 0, the
// cylinders' seam at `angle` from the top (z = kR) towards +y, the
// cylinder one face.
TopoDS_Shape bar_with_boss(double h, double angle) {
  const gp_Dir x(1., 0., 0.), seam(0., std::sin(angle), std::cos(angle));
  const TopoDS_Shape bar =
    BRepAlgoAPI_Fuse(BRepPrimAPI_MakeBox(gp_Pnt(0., -kR, -10.), gp_Pnt(kBar, kR, 0.)).Shape(),
                     BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0., 0., 0.), x, seam), kR, kBar).Shape())
      .Shape();
  const TopoDS_Shape fused =
    BRepAlgoAPI_Fuse(bar, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-h, 0., 0.), x, seam), kR, h + 1.).Shape())
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(fused, true, true, true);
  unify.Build();
  return unify.Shape();
}

// The boss's foot (the circle's part at x = 0 below the top) and rim (the
// circle at x = -h).
std::vector<TopoDS_Edge> foot_and_rim(const TopoDS_Shape& body, double h) {
  std::vector<TopoDS_Edge> out;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    BRepAdaptor_Curve  c(e);
    if (c.GetType() != GeomAbs_Circle) {
      continue;
    }
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(m.X() + h) < 1e-9 || (std::abs(m.X()) < 1e-9 && m.Z() < -1e-9)) {
      bool seen = false;
      for (const TopoDS_Edge& o : out) {
        seen = seen || o.IsSame(e);
      }
      if (!seen) {
        out.push_back(e);
      }
    }
  }
  return out;
}

double volume(const TopoDS_Shape& s) {
  GProp_GProps p;
  BRepGProp::VolumeProperties(s, p);
  return p.Mass();
}

// The area of the faces on cylinders of radius kR (the bar's top and the
// boss).
double cylinder_area(const TopoDS_Shape& s) {
  double area = 0.;
  for (TopExp_Explorer ex(s, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face&  f = TopoDS::Face(ex.Current());
    BRepAdaptor_Surface a(f, false);
    if (a.GetType() == GeomAbs_Cylinder && std::abs(a.Cylinder().Radius() - kR) < 1e-9) {
      GProp_GProps p;
      BRepGProp::SurfaceProperties(f, p);
      area += p.Mass();
    }
  }
  return area;
}

// Whether every edge of the shape lies on two faces.
bool closed(const TopoDS_Shape& shape) {
  EdgeFaces map;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, map);
  for (int i = 1; i <= map.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(map.FindKey(i))) && map(i).Extent() != 2) {
      return false;
    }
  }
  return true;
}

// The body rounded with r along the boss's foot and rim; null where the
// fillet fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, double h, double r) {
  const std::vector<TopoDS_Edge> edges = foot_and_rim(body, h);
  CHECK(edges.size() >= 2);
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& e : edges) {
    fillet.Add(r, e);
  }
  fillet.Build();
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

void check_boss(double h, double seam) {
  std::printf("h %.2f, seam at %.4f\n", h, seam);
  const TopoDS_Shape body = bar_with_boss(h, seam);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const double r = 0.5 * h;
  // A little smaller, the cylinder keeps a strip between the contacts; the
  // volume runs on smoothly to the exact size (its second differences at
  // steps of 0.001 are about 5e-6 mm3).
  std::vector<double> smaller;
  for (const double d : {0.002, 0.001}) {
    const TopoDS_Shape s = rounded(body, h, r - d);
    CHECK(!s.IsNull());
    if (s.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(s).IsValid());
    smaller.push_back(volume(s));
  }
  const TopoDS_Shape exact = rounded(body, h, r);
  CHECK(!exact.IsNull());
  if (exact.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(exact).IsValid());
  CHECK(closed(exact));
  const double expected = 2. * smaller[1] - smaller[0];
  std::printf("  volume %.6f, from the smaller radii %.6f\n", volume(exact), expected);
  CHECK(std::abs(volume(exact) - expected) <= 2e-5);
  // The cylinder: the bar's top from the rim's contact on (x = -h / 2), the
  // boss's part below the top gone with the foot's and the rim's blends.
  CHECK_NEAR_TOL(cylinder_area(exact), kPi * kR * (kBar + 0.5 * h), 1e-7);
}

// The seam on the top: the foot's contact a part inside the rim's line.
void test_seam_on_the_top() {
  check_boss(2., 0.);
  check_boss(2.5, 0.);
  // Well smaller, as before.
  const TopoDS_Shape s = rounded(bar_with_boss(2.5, 0.), 2.5, 1.2);
  CHECK(!s.IsNull() && BRepCheck_Analyzer(s).IsValid());
}

// The seam at an end of the foot, where the rim's closed contact starts
// and where the foot's end runs along the seam.
void test_seam_at_the_foot_end() {
  check_boss(2., 0.5 * kPi);
  check_boss(2., -0.5 * kPi);
  check_boss(2.5, 0.5 * kPi);
}

// The seam below the top: the foot's contact in two pieces, each riding on
// its part of the rim's.
void test_seam_below_the_top() {
  check_boss(2., kPi);
  check_boss(2.5, 2.5);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_seam_on_the_top", test::test_seam_on_the_top);
  test::guarded("test_seam_at_the_foot_end", test::test_seam_at_the_foot_end);
  test::guarded("test_seam_below_the_top", test::test_seam_below_the_top);
  if (test::failures != 0) {
    std::printf("%d check(s) failed\n", test::failures);
  }
  return test::failures == 0 ? 0 : 1;
}
