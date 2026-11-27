// SPDX-License-Identifier: MIT
// A block's edge rounded up to a vertex where its wall steps back into the
// material by much less than the radius, built with OCCT's fillet alone:
// the wall's face at the edge ends on a narrow strip (the step's face,
// square to the rounding), beyond which the wall goes on a few micrometres
// further in. OCCT found no face to end the rounding on and left the shape
// invalid. The rounding now ends at the section through the vertex and its
// ball turns on about its contact's end until it lies in the wall's plane,
// a quarter torus cut by the stepped-back wall. A stored design with a
// rim whose wall steps back by 4.3 micrometres is the case behind the patch
// 0951 of the OCCT port (third_party/vcpkg-ports, mitcad#343). Patch 0962
// builds that end whichever side of the blend the wall is on (the body's
// faces in the reverse order) and runs a later rounding on across the step
// where an earlier one ends the recess.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kStep = 0.0043; // how far the wall steps back
constexpr double kRound = 0.3;   // the rounding's radius
constexpr double kLength = 8.0;  // the rounded edge's length

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// Simpson's rule on [a, b] in n (even) steps.
template <typename F> double simpson(F&& f, double a, double b, int n) {
  const double h = (b - a) / n;
  double sum = f(a) + f(b);
  for (int i = 1; i < n; ++i) {
    sum += f(a + i * h) * (i % 2 == 1 ? 4 : 2);
  }
  return sum * h / 3;
}

// The material one rounded rim loses: the rounding's section, the square
// of the radius less the quarter disk, along the edge, and at the end what
// the turning ball takes. At the distance d from the wall's plane of the
// ball's centres, the torus leaves a quarter disk of radius
// a = R - sqrt(R^2 - d^2) about the contact's end point, of which only the
// part beyond the stepped-back wall is material.
double rim_loss() {
  const double along = kLength * (1 - kPi / 4) * kRound * kRound;
  const double end = simpson(
    [](double d) {
      const double a = kRound - std::sqrt(std::max(0.0, kRound * kRound - d * d));
      if (a <= kStep) {
        return 0.0;
      }
      // (The quarter disk's area beyond y = kStep.)
      const double t = std::acos(kStep / a);
      return 0.5 * a * a * (t - std::sin(t) * std::cos(t));
    },
    0.0, kRound, 4000);
  return along + end;
}

// The block x in [-7, 7], y in [0, 10], z in [15, 30] (mirrored: z in
// [-30, -15]), its wall y = 0 stepped back by kStep over z in [21, 22]
// (above: [23, 24]); the edges of the wall with the side x = 7 (both: and
// x = -7) on the longer side of the step rounded.
struct Case {
  bool above;    // the step above the rounded edge, at its other end
  bool both;     // both sides' edges rounded
  bool mirror;   // the block mirrored in z = 0
  bool reversed; // the body's faces in the reverse order
};

// The same solid with its faces in the reverse order: OCCT takes a
// rounded edge's faces in the order it meets them, so that the rounding's
// sides, and the blend's surface's direction across it, swap.
TopoDS_Shape faces_reversed(const TopoDS_Shape& body) {
  std::vector<TopoDS_Shape> faces;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    faces.push_back(ex.Current());
  }
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (auto it = faces.rbegin(); it != faces.rend(); ++it) {
    builder.Add(shell, *it);
  }
  shell.Closed(true);
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  builder.Add(solid, shell);
  return solid;
}

void check_case(const Case& c) {
  const double s = c.mirror ? -1.0 : 1.0;
  const TopoDS_Shape block =
    BRepPrimAPI_MakeBox(gp_Pnt(-7, 0, c.mirror ? -30 : 15), gp_Pnt(7, 10, c.mirror ? -15 : 30)).Shape();
  const double z0 = c.above ? 23 : 21;
  const double n0 = std::min(s * z0, s * (z0 + 1)), n1 = std::max(s * z0, s * (z0 + 1));
  const TopoDS_Shape notch = BRepPrimAPI_MakeBox(gp_Pnt(-8, -1, n0), gp_Pnt(8, kStep, n1)).Shape();
  const TopoDS_Shape cut = BRepAlgoAPI_Cut(block, notch).Shape();
  const TopoDS_Shape body = c.reversed ? faces_reversed(cut) : cut;
  BRepFilletAPI_MakeFillet fillet(body);
  int added = 0;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
    const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
    const bool side = std::abs(a.X() - 7) < 1e-9 || (c.both && std::abs(a.X() + 7) < 1e-9);
    const bool along = std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y()) < 1e-9 && std::abs(b.Y()) < 1e-9;
    const double zmid = s * 0.5 * (a.Z() + b.Z());
    const bool longer = c.above ? zmid < z0 : zmid > z0;
    if (side && along && longer) {
      fillet.Add(kRound, TopoDS::Edge(edges(i)));
      ++added;
    }
  }
  CHECK(added == (c.both ? 2 : 1));
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
  }
  std::printf("step %s, %s, %s%s:", c.above ? "above" : "below", c.both ? "both rims" : "one rim",
              c.mirror ? "mirrored" : "as built", c.reversed ? ", faces reversed" : "");
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    std::printf(" not done\n");
    return;
  }
  const TopoDS_Shape rounded = fillet.Shape();
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  int tori = 0;
  for (TopExp_Explorer ex(rounded, TopAbs_FACE); ex.More(); ex.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(ex.Current(), props);
    CHECK(std::abs(props.Mass()) > 1e-9); // no face of no area
    if (BRepAdaptor_Surface(TopoDS::Face(ex.Current())).GetType() == GeomAbs_Torus) {
      ++tori;
    }
  }
  CHECK(tori == (c.both ? 2 : 1)); // each rounding's end its quarter torus
  const double loss = volume_of(body) - volume_of(rounded);
  const double want = (c.both ? 2 : 1) * rim_loss();
  std::printf(" material taken %.7f, %.7f wanted\n", loss, want);
  CHECK(std::abs(loss - want) < 2e-6);
}

void test_rounding_ends_at_a_step_back_of_its_wall() {
  for (const bool above : {false, true}) {
    for (const bool both : {false, true}) {
      for (const bool mirror : {false, true}) {
        for (const bool reversed : {false, true}) {
          check_case({above, both, mirror, reversed});
        }
      }
    }
  }
}

// The edge on the step's other side, rounded after the first with a
// smaller radius: the recess beyond its end is closed at its far end by the
// first rounding's torus, not by a strip back to the wall, and the blend
// runs on across the step (cutting the recess's few micrometres) until that
// torus cuts it, as the stored design's next rounding there does; it does
// not turn about its end.
void test_second_rounding_runs_on_across_the_step() {
  for (const bool reversed : {false, true}) {
    const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-7, 0, 15), gp_Pnt(7, 10, 30)).Shape();
    const TopoDS_Shape notch = BRepPrimAPI_MakeBox(gp_Pnt(-8, -1, 21), gp_Pnt(8, kStep, 22)).Shape();
    const TopoDS_Shape cut = BRepAlgoAPI_Cut(block, notch).Shape();
    const TopoDS_Shape body = reversed ? faces_reversed(cut) : cut;
    // The wall's edge with x = 7 above the step (first, kRound) or below
    // it (second, the smaller radius).
    const auto round_wall_edge = [](const TopoDS_Shape& shape, bool above, double r) {
      BRepFilletAPI_MakeFillet fillet(shape);
      NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
      TopExp::MapShapes(shape, TopAbs_EDGE, edges);
      for (int i = 1; i <= edges.Extent(); ++i) {
        BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
        const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
        if (e.GetType() == GeomAbs_Line && std::abs(a.X() - 7) < 1e-9 && std::abs(b.X() - 7) < 1e-9
            && std::abs(a.Y()) < 1e-9 && std::abs(b.Y()) < 1e-9 && (0.5 * (a.Z() + b.Z()) > 22) == above) {
          fillet.Add(r, TopoDS::Edge(edges(i)));
        }
      }
      try {
        OCC_CATCH_SIGNALS
        fillet.Build();
      } catch (const Standard_Failure& e) {
        std::printf("fillet threw: %s\n", e.what());
      }
      return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
    };
    const TopoDS_Shape first = round_wall_edge(body, true, kRound);
    CHECK(!first.IsNull());
    if (first.IsNull()) {
      continue;
    }
    const TopoDS_Shape second = round_wall_edge(first, false, 0.2);
    CHECK(!second.IsNull());
    if (second.IsNull()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(second).IsValid());
    int tori = 0;
    double reach = 0; // how far up the second rounding's cylinder runs
    for (TopExp_Explorer ex(second, TopAbs_FACE); ex.More(); ex.Next()) {
      const BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
      if (s.GetType() == GeomAbs_Torus) {
        ++tori;
      } else if (s.GetType() == GeomAbs_Cylinder && std::abs(s.Cylinder().Radius() - 0.2) < 1e-9) {
        Bnd_Box box;
        BRepBndLib::Add(ex.Current(), box);
        double x0, y0, z0, x1, y1, z1;
        box.Get(x0, y0, z0, x1, y1, z1);
        reach = std::max(reach, z1);
      }
    }
    std::printf("second rounding%s: tori %d, its cylinder up to z %.4f\n", reversed ? ", faces reversed" : "",
                tori, reach);
    CHECK(tori == 1);     // the first rounding's only
    CHECK(reach > 21.5);  // past the step at z = 21, up to the first's torus
    CHECK(reach < 22.0);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_step_end) {
  test::guarded("test_rounding_ends_at_a_step_back_of_its_wall",
                test::test_rounding_ends_at_a_step_back_of_its_wall);
  test::guarded("test_second_rounding_runs_on_across_the_step",
                test::test_second_rounding_runs_on_across_the_step);
  if (test::failures == 0) {
    std::puts("test_fillet_step_end: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
