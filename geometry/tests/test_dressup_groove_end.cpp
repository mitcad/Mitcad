// SPDX-License-Identifier: MIT
// The end of a shallow swept groove rounded at a ball-mill radius
// (mitcad#2154): a block with a groove cut by a circle (radius Rc 10, 1.5
// deep) swept along a straight line (or an arc), ending in a flat end cap
// inside the block; the edge between the end cap and the groove's floor
// filleted at a radius r wider than the end cap is high, so that the
// rounding takes the end cap away and ends on the block's top face
// (mitcad#1819's rule).
//
// - r < Rc: the rounding is a piece of a torus (the ball's centre on a
//   circle of radius Rc - r about the groove's axis, r from the end cap's
//   plane), r = Rc a piece of a sphere (the ball's centre on the axis, not
//   moving along the edge). Each: one valid closed solid, the end cap gone,
//   a face named R:fillet(…), and for the straight groove its volume within
//   1e-6 relative of the reference: the block less the groove whose end is
//   the region of revolution about its axis (radius Rc up to r before the
//   end cap, then (Rc - r) + sqrt(r^2 - z^2) over the last r), integrated
//   in its sections.
// - r > Rc: no ball of that radius fits in the groove; a failure in words
//   that says so, no names in what a person reads, never that the result
//   is not a closed solid.
//
//   test_dressup_groove_end

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The groove: a circle of radius kRc whose centre lies kAxis above the
// block's bottom, the block kTop high: kDepth deep.
constexpr double kRc = 10.0;
constexpr double kTop = 10.0;
constexpr double kDepth = 1.5;
constexpr double kAxis = kTop + kRc - kDepth; // 18.5
// The straight groove's end cap at y = kEnd; the block from y = 0 to 60,
// x from -15 to 15.
constexpr double kEnd = 40.0;
// The arc groove's path: a circle of radius kPath about the vertical axis
// through (kPath, 0), from -0.1 rad (outside the block, y > 0) to kSweep
// rad, its end cap there.
constexpr double kPath = 40.0;
constexpr double kSweep = 0.6;

// The body with its faces named N1, N2, ... in TopExp order.
ShapePtr named(const TopoDS_Shape& body) {
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"N" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(body, faces);
}

double volume_of(const TopoDS_Shape& shape) { return mitcad::analysis::physical_properties(shape).volume; }

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

bool closed_valid_solid(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1 && BRepCheck_Analyzer(shape).IsValid();
}

bool starts_with(const std::string& text, const std::string& prefix) {
  return text.compare(0, prefix.size(), prefix) == 0;
}

int faces_starting(const Shape& shape, const std::string& prefix) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    const NameList& names = shape.face_names(f);
    count += std::any_of(names.begin(), names.end(), [&](const std::string& n) { return starts_with(n, prefix); })
                 ? 1
                 : 0;
  }
  return count;
}

// Whether a face of the shape is the face `name` (or a piece of it): a
// face with that name and none of the rounding's (`made`, its prefix). (A
// face the rounding takes away gives its names to the rounding's face
// there.)
bool holds_face(const Shape& shape, const std::string& name, const std::string& made) {
  for (int f = 0; f < shape.face_count(); ++f) {
    const NameList& names = shape.face_names(f);
    const bool of_rounding =
        std::any_of(names.begin(), names.end(), [&](const std::string& n) { return starts_with(n, made); });
    if (!of_rounding &&
        std::any_of(names.begin(), names.end(), [&](const std::string& n) { return name_matches(n, name); })) {
      return true;
    }
  }
  return false;
}

// The block with the groove, straight (along +y, the end cap the plane
// y = kEnd) or along the arc (the end cap the plane through the vertical
// axis at kSweep rad). The circle's seam lies at its top, outside the
// block.
TopoDS_Shape grooved(bool arc) {
  if (!arc) {
    const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-15, 0, 0), gp_Pnt(15, 60, kTop)).Shape();
    const TopoDS_Shape tool =
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -1, kAxis), gp_Dir(0, 1, 0), gp_Dir(0, 0, 1)), kRc, kEnd + 1)
            .Shape();
    return BRepAlgoAPI_Cut(block, tool).Shape();
  }
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-20, -40, 0), gp_Pnt(40, 0, kTop)).Shape();
  // (Angle 0 at (0, 0, kAxis), increasing into -y.)
  const TopoDS_Shape ring =
      BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(kPath, 0, kAxis), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), kPath, kRc,
                            kSweep + 0.1)
          .Shape();
  gp_Trsf back;
  back.SetRotation(gp_Ax1(gp_Pnt(kPath, 0, 0), gp_Dir(0, 0, 1)), -0.1);
  const TopoDS_Shape tool = BRepBuilderAPI_Transform(ring, back, true).Shape();
  return BRepAlgoAPI_Cut(block, tool).Shape();
}

// The end cap's plane: a point on it and its normal (into the groove).
gp_Pnt cap_point(bool arc) {
  if (!arc) {
    return gp_Pnt(0, kEnd, kAxis);
  }
  return gp_Pnt(kPath - kPath * std::cos(kSweep), -kPath * std::sin(kSweep), kAxis);
}

// The area of the part below the block's top of a disc of radius rho whose
// centre lies kRc - kDepth above the top.
double segment_below(double rho) {
  const double h = kRc - kDepth;
  if (rho <= h) {
    return 0.0;
  }
  return rho * rho * std::acos(h / rho) - h * std::sqrt(rho * rho - h * h);
}

// The straight groove's reference: the block less the groove whose end is
// the region of revolution about its axis, radius Rc up to r before the
// end cap, then (Rc - r) + sqrt(r^2 - z^2) over the last r (Simpson's rule
// in 400000 pieces over that last r).
double reference_volume(double r) {
  const double block = 30.0 * 60.0 * kTop;
  double removed = (kEnd - r) * segment_below(kRc);
  const int n = 400000;
  const double h = r / n;
  double sum = 0.0;
  for (int i = 0; i <= n; ++i) {
    const double z = std::min(r, i * h);
    const double rho = (kRc - r) + std::sqrt(std::max(0.0, r * r - z * z));
    const double weight = i == 0 || i == n ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0);
    sum += weight * segment_below(rho);
  }
  removed += sum * h / 3.0;
  return block - removed;
}

// The end cap (planar, on the cap's plane) and its edge on the floor.
struct Groove {
  ShapePtr body;
  std::string cap;
  std::string floor;
  std::string edge;
};

Groove groove(bool arc) {
  Groove g;
  g.body = named(grooved(arc));
  const gp_Pnt at = cap_point(arc);
  for (int f = 0; f < g.body->face_count(); ++f) {
    const BRepAdaptor_Surface surface(g.body->face(f), false);
    if (surface.GetType() == GeomAbs_Plane && surface.Plane().Distance(at) < 1e-6) {
      g.cap = g.body->face_names(f).front();
    }
    if (surface.GetType() == GeomAbs_Cylinder || surface.GetType() == GeomAbs_Torus) {
      g.floor = g.body->face_names(f).front();
    }
  }
  for (int e = 0; e < g.body->edge_count(); ++e) {
    const BRepAdaptor_Curve curve(g.body->edge(e));
    if (curve.GetType() != GeomAbs_Circle) {
      continue;
    }
    // (The circle of radius Rc about the end cap's centre, below the top.)
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (curve.Circle().Location().Distance(at) < 1e-6 && std::abs(curve.Circle().Radius() - kRc) < 1e-6 &&
        m.Z() < kTop) {
      g.edge = g.body->edge_name(e);
    }
  }
  return g;
}

// The geometry kernel's own rounding of the edge (OCCT's fillet without
// the facade's ways round a failure), printed for a row that fails.
void plain_rounding(const Groove& g, double r) {
  try {
    OCC_CATCH_SIGNALS
    BRepBuilderAPI_Copy copy(g.body->occt());
    BRepFilletAPI_MakeFillet maker(copy.Shape());
    for (const int e : g.body->find_edges(g.edge)) {
      maker.Add(r, TopoDS::Edge(copy.ModifiedShape(g.body->edge(e))));
    }
    maker.Build();
    if (!maker.IsDone()) {
      std::fprintf(stderr, "  the kernel's plain rounding: fails (status %d)\n", static_cast<int>(maker.StripeStatus(1)));
      return;
    }
    std::fprintf(stderr, "  the kernel's plain rounding: builds, valid closed %d, volume %.9f\n",
                 closed_valid_solid(maker.Shape()) ? 1 : 0, volume_of(maker.Shape()));
  } catch (const std::exception& error) {
    // (OCCT's failures too.)
    std::fprintf(stderr, "  the kernel's plain rounding: throws %s\n", error.what());
  }
}

void run_row(bool arc, double ratio) {
  const double r = ratio * kRc;
  std::fprintf(stderr, "%s groove, r = %g Rc\n", arc ? "arc" : "straight", ratio);
  const Groove g = groove(arc);
  CHECK(!g.cap.empty());
  CHECK(!g.floor.empty());
  CHECK(!g.edge.empty());
  if (g.cap.empty() || g.edge.empty()) {
    return;
  }
  ShapePtr result;
  std::string error;
  try {
    FilletSet set;
    set.edges = {g.edge};
    set.radius = r;
    result = fillet("R", *g.body, {set});
  } catch (const std::exception& e) {
    error = e.what();
  }
  if (!result) {
    std::fprintf(stderr, "  failed: %s\n", error.c_str());
    plain_rounding(g, r);
    CHECK(result != nullptr);
    return;
  }
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "  note: %s\n", note.c_str());
  }
  double smallest = 1e300;
  for (int f = 0; f < result->face_count(); ++f) {
    smallest = std::min(smallest, area_of(result->face(f)));
  }
  const double volume = volume_of(result->occt());
  std::fprintf(stderr, "  built: volume %.9f (the body's %.9f), smallest face %.3g mm2\n", volume,
               volume_of(g.body->occt()), smallest);
  CHECK(closed_valid_solid(result->occt()));
  CHECK(smallest > 0.01);
  CHECK(faces_starting(*result, "R:fillet(") >= 1);
  // The end cap gone, the floor kept; a rounding of a concave edge adds.
  CHECK(!holds_face(*result, g.cap, "R:fillet("));
  CHECK(holds_face(*result, g.floor, "R:fillet("));
  CHECK(volume > volume_of(g.body->occt()));
  if (!arc) {
    CHECK_NEAR(volume, reference_volume(r));
  }
}

// r > Rc: no ball of that radius fits in the groove.
void run_too_large(bool arc, double ratio) {
  const double r = ratio * kRc;
  std::fprintf(stderr, "%s groove, r = %g Rc (too large)\n", arc ? "arc" : "straight", ratio);
  const Groove g = groove(arc);
  CHECK(!g.edge.empty());
  if (g.edge.empty()) {
    return;
  }
  std::string error;
  try {
    FilletSet set;
    set.edges = {g.edge};
    set.radius = r;
    (void)fillet("R", *g.body, {set});
  } catch (const std::exception& e) {
    error = e.what();
  }
  CHECK(!error.empty());
  const std::string shown = error_message(error);
  std::fprintf(stderr, "  failed: %s\n", shown.c_str());
  CHECK(shown.find("no ball of that radius fits in the groove") != std::string::npos);
  // (No names: neither an edge's nor a face's, N1, N2, … here.)
  bool names = shown.find("E{") != std::string::npos;
  for (std::size_t i = 0; i + 1 < shown.size(); ++i) {
    names = names || (shown[i] == 'N' && shown[i + 1] >= '0' && shown[i + 1] <= '9');
  }
  CHECK(!names);
  CHECK(shown.find("not a closed solid") == std::string::npos);
  // (The edge as its place.)
  CHECK(error.find(g.edge) != std::string::npos);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(dressup_groove_end) {
  // The groove end's build says on the standard error why it gives up
  // (MITCAD_TRACE_GROOVE_END), so that a failing row's log tells which
  // check refused it.
#ifdef _WIN32
  _putenv_s("MITCAD_TRACE_GROOVE_END", "1");
#else
  setenv("MITCAD_TRACE_GROOVE_END", "1", 1);
#endif
  for (const double ratio : {0.5, 0.99, 1.0}) {
    const std::string name = "straight groove, r = " + std::to_string(ratio) + " Rc";
    test::guarded(name.c_str(), [&] { test::run_row(false, ratio); });
  }
  for (const double ratio : {0.5, 1.0}) {
    const std::string name = "arc groove, r = " + std::to_string(ratio) + " Rc";
    test::guarded(name.c_str(), [&] { test::run_row(true, ratio); });
  }
  test::guarded("straight groove, r = 1.01 Rc", [] { test::run_too_large(false, 1.01); });
  test::guarded("arc groove, r = 1.01 Rc", [] { test::run_too_large(true, 1.01); });
  if (test::failures == 0) {
    std::puts("test_dressup_groove_end: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
