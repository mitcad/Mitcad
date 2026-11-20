// SPDX-License-Identifier: MIT
// Windows in a ring wall 3 thick, each window's floor and ceiling rounded
// with 1.5 on both edges, built with OCCT's fillet alone: the two roundings
// of a floor or a ceiling take the whole face away, their contacts on it
// running along each other at the wall's middle, and end at the pillars'
// full-rounded ends. OCCT took the two contacts for roundings that clash,
// or left the face as a sliver between them where the two corners at an end
// made a point each. A ring of a stored design with the windows of a
// circular pattern is the case behind the patch 0950 of the OCCT port
// (third_party/vcpkg-ports, mitcad#343). Each rounding is first tried over
// its floor's or ceiling's surface unbounded (patch 0940), which the
// roundings' analytic pieces refuse; walking that try's guides past the
// spine's ends first took most of the fillet's time (patch 1696,
// mitcad#1824, mitcad#2065).

#include <chrono>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kInner = 112.0;  // the wall's inner radius
constexpr double kOuter = 115.0;  // its outer radius
constexpr double kMiddle = 113.5; // the radius of its middle
constexpr double kEnd = 1.5;      // the pillars' full rounds, half the wall
constexpr double kRound = 1.5;    // the windows' roundings, half the wall
constexpr int kWindows = 5;

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The angle of a pillar end's axis from the window's middle.
double end_angle() { return std::atan2(16.5, std::sqrt(kMiddle * kMiddle - 16.5 * 16.5)); }

// The ring wall from z = -30 to 0 with kWindows windows from z = -20 to -5,
// each between two pillars whose ends are half cylinders tangent to both
// walls, each end two faces of its cylinder that meet at the end's tip.
TopoDS_Shape ring_with_windows() {
  const gp_Ax2 axis(gp_Pnt(0, 0, -30), gp_Dir(0, 0, 1));
  TopoDS_Shape ring = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(axis, kOuter, 30).Shape(),
                                      BRepPrimAPI_MakeCylinder(axis, kInner, 30).Shape())
                          .Shape();
  const double phi = end_angle();
  const gp_Ax2 at(gp_Pnt(0, 0, -20), gp_Dir(0, 0, 1), gp_Dir(std::sin(phi), std::cos(phi), 0));
  TopoDS_Shape window = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(at, kOuter + 1, 15, 2 * phi).Shape(),
                                        BRepPrimAPI_MakeCylinder(at, kInner - 1, 15, 2 * phi).Shape())
                            .Shape();
  for (const int s : {-1, 1}) {
    const gp_Pnt centre(kMiddle * std::sin(s * phi), kMiddle * std::cos(phi), -20);
    const gp_Dir towards(-s, 0, 0);
    for (const gp_Dir& start : {towards, towards.Reversed()}) {
      const gp_Ax2 half(centre, gp_Dir(0, 0, 1), start);
      window = BRepAlgoAPI_Cut(window, BRepPrimAPI_MakeCylinder(half, kEnd, 15, kPi).Shape()).Shape();
    }
  }
  TopoDS_Shape cut = ring;
  for (int k = 0; k < kWindows; ++k) {
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 2 * kPi * k / kWindows);
    cut = BRepAlgoAPI_Cut(cut, BRepBuilderAPI_Transform(window, turn, true).Shape()).Shape();
  }
  // The faces of one surface joined but for the ends' halves, kept apart at
  // their tips (the lines at the end's radius from its axis towards the
  // window's middle).
  ShapeUpgrade_UnifySameDomain unify(cut, true, true, true);
  for (TopExp_Explorer ex(cut, TopAbs_EDGE); ex.More(); ex.Next()) {
    BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
    if (c.GetType() != GeomAbs_Line || std::abs(a.X() - b.X()) > 1e-9 || std::abs(a.Y() - b.Y()) > 1e-9) {
      continue;
    }
    for (int k = 0; k < kWindows; ++k) {
      gp_Trsf back;
      back.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), -2 * kPi * k / kWindows);
      const gp_Pnt a0 = a.Transformed(back);
      if (std::abs(std::abs(a0.X()) - (16.5 - kEnd)) < 1e-6 && std::abs(a0.Y() - kMiddle * std::cos(phi)) < 1e-6) {
        unify.KeepShape(ex.Current());
      }
    }
  }
  unify.Build();
  return unify.Shape();
}

// The windows' floors' and ceilings' edges with both walls.
std::vector<TopoDS_Edge> window_edges(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> found;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve c(TopoDS::Edge(edges(i)));
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    const double r = std::hypot(m.X(), m.Y());
    const bool sill = std::abs(m.Z() + 20) < 1e-9 || std::abs(m.Z() + 5) < 1e-9;
    if (sill && c.GetType() == GeomAbs_Circle && (std::abs(r - kInner) < 1e-9 || std::abs(r - kOuter) < 1e-9)) {
      found.push_back(TopoDS::Edge(edges(i)));
    }
  }
  return found;
}

// The material one rounded floor loses: under the floor's outline (the
// sector between the pillar ends' axes less their half disks), down to the
// torus of the two roundings, 1.5 - sqrt(1.5^2 - (r - 113.5)^2) deep.
double sill_loss() {
  const auto depth = [](double r) {
    const double d = r - kMiddle;
    return kRound - std::sqrt(std::max(0.0, kRound * kRound - d * d));
  };
  // Simpson's rule on [a, b] in n (even) steps.
  const auto simpson = [](auto&& f, double a, double b, int n) {
    const double h = (b - a) / n;
    double sum = f(a) + f(b);
    for (int i = 1; i < n; ++i) {
      sum += f(a + i * h) * (i % 2 == 1 ? 4 : 2);
    }
    return sum * h / 3;
  };
  // (Across the wall at r = kMiddle + kRound sin(u), smooth in u.)
  const double sector =
    2 * end_angle() * simpson(
                        [&](double u) {
                          const double r = kMiddle + kRound * std::sin(u);
                          return kRound * (1 - std::cos(u)) * r * kRound * std::cos(u);
                        },
                        -kPi / 2, kPi / 2, 2000);
  // A half disk about the middle radius, in polar coordinates about its
  // centre (the depth by the distance from the ring's axis).
  const double half_disk = simpson(
    [&](double rho) {
      return rho * simpson(
                     [&](double t) {
                       // (The disk's centre at (0, kMiddle), t from its axis
                       // round the half that stands into the window.)
                       const double x = rho * std::cos(t), y = kMiddle + rho * std::sin(t);
                       return depth(std::hypot(x, y));
                     },
                     -kPi / 2, kPi / 2, 400);
    },
    0.0, kEnd, 400);
  return sector - 2 * half_disk;
}

void test_windows_sills_rounded_away() {
  const TopoDS_Shape body = ring_with_windows();
  const std::vector<TopoDS_Edge> edges = window_edges(body);
  CHECK(edges.size() == 4 * kWindows);
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& e : edges) {
    fillet.Add(kRound, e);
  }
  const auto start = std::chrono::steady_clock::now();
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
  }
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("fillet: %.2f s\n", seconds);
  // (About 3 s with patch 1696, 21 to 34 s without it: the tries over the
  // unbounded faces walked and approximated past the spine's ends in vain.)
  CHECK(seconds < 15.0);
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape rounded = fillet.Shape();
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  // Every floor and ceiling gone, each its own torus's two halves.
  for (TopExp_Explorer ex(rounded, TopAbs_FACE); ex.More(); ex.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(ex.Current(), props);
    CHECK(std::abs(props.Mass()) > 1e-6); // no face of no area
  }
  const double loss = volume_of(body) - volume_of(rounded);
  const double want = 2 * kWindows * sill_loss();
  std::printf("material taken: %.6f, %.6f wanted\n", loss, want);
  CHECK(std::abs(loss - want) < 2e-3);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_windows_sills_rounded_away", test::test_windows_sills_rounded_away);
  if (test::failures == 0) {
    std::puts("test_fillet_consumed_strip: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
