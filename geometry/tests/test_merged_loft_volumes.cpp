// SPDX-License-Identifier: MIT
// The volume and area MitCad reports for lofts with Tangent Edges Merge
// and for their shells, within 1e-4 (relative) of a reference integration
// and within 5e-3 of a mesh (mitcad#2309).
//
// A loft through two sections merged into one smooth edge each has one
// side face on a periodic surface that is only C1 continuous where the
// sections' lines and arcs joined, and the inside wall of its shell is an
// offset of that surface. OCCT's default integration (BRepGProp's
// VolumeProperties without an accuracy) takes a face with one set of Gauss
// points over its whole parameter range and an offset surface without its
// basis's knots: it measured the loft from a circle of radius 50 to a slot
// 188 x 42 some 11 % low (711 763 mm3 for 800 657) and the 2 mm inside
// wall of its shell negative (-10 460 mm3 for some 85 900), so that Shell
// called the wall empty (mitcad#2271; Shell's check now measures again on
// a copy split at the C1 lines, geometry/src/faceops.cpp, split_volume).
//
// What MitCad reports (geometry::mass_properties for the bodies query,
// Inspect and the properties; analysis::physical_properties for Physical
// Properties and the mass) is the analysis library's integration, span by
// span between every knot of the surface and of its offset's basis
// (geometry/analysis/src/face_integral.cpp), which needs no split. This
// test checks it for the merged lofts (circles to slots, the owner's among
// them, and a circle to a closed spline outline) and their 2 mm shells
// Inside and Outside with the circle's end removed, against two measures
// of its own. OCCT's default rule is printed beside, not checked.
//
// The reference, within 1e-4 as mitcad#2309 asks: OCCT's adaptive
// Gauss-Kronrod integration to 1e-8 of a copy whose faces are split where
// their surfaces are less than C2 continuous (as Shell's check splits
// them), every piece smooth; its own error estimate must be under 1e-5.
//
// The mesh, within 5e-3: a measure owing nothing to either integration.
// Each triangle lies within the deflection of the surface, on average some
// two thirds of it, so a mesh's volume is off by up to about 2/3 deflection
// x curved area: with 5e-3 mm under 4e-4 of a loft and under 3.4e-3 of a
// 2 mm wall even where its two sides' errors added up (they mostly cancel:
// the outer side's mesh lies inside the material, the inner side's outside
// it). It cannot confirm 1e-4 at a cost a ctest can bear: a mesh fine
// enough for that (5e-5 mm) has tens of millions of triangles on these
// walls and ran for minutes, killed for its time or memory; at 5e-3 mm it
// has some 1e5. It still shows OCCT's default rule's errors (11 % on a
// loft, a wall's sign) many times over.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_Shape.hxx>
#include <Geom_Surface.hxx>
#include <ShapeUpgrade_ShapeDivideContinuity.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

// The reported measures against the reference (relative), as mitcad#2309
// asks; the reference's accuracy and the bound on its error estimate.
constexpr double kAgreement = 1.0e-4;
constexpr double kReferenceEps = 1.0e-8;
constexpr double kReferenceError = 1.0e-5;
// The mesh's deflection (mm), its angular deflection (rad; the linear one
// governs every curved face here) and the reported measures against it.
constexpr double kDeflection = 5.0e-3;
constexpr double kAngle = 0.5;
constexpr double kMeshAgreement = 5.0e-3;

double relative(double value, double reference) {
  return std::abs(value - reference) / std::max(std::abs(reference), 1e-300);
}

g::Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

g::Segment half_arc(const std::string& name, double cx, double cy, double radius, double from) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Arc;
  s.center = gp_Pnt2d(cx, cy);
  s.radius = radius;
  s.start_angle = from;
  s.end_angle = from + kPi;
  return s;
}

g::Region region_of(const std::string& name, const std::vector<g::Segment>& segments) {
  g::Region region;
  region.name = name;
  region.loops.push_back(g::Loop{segments});
  return region;
}

// A circle of `radius` about the sketch origin.
g::Region circle(double radius) {
  g::Segment s;
  s.name = "c1";
  s.kind = g::SegmentKind::Circle;
  s.center = gp_Pnt2d(0, 0);
  s.radius = radius;
  return region_of("r{c1}", {s});
}

// A centre-point slot along the sketch's x axis about its origin, its arcs'
// centres `half` either side of it, of radius `radius`: lines and half
// circles meeting tangentially, which Merge joins into one edge.
g::Region slot(double half, double radius) {
  return region_of("r{c2,c3,c4,c5}",
                   {line("c2", -half, -radius, half, -radius), half_arc("c3", half, 0, radius, -kPi / 2),
                    line("c4", half, radius, -half, radius), half_arc("c5", -half, 0, radius, kPi / 2)});
}

// A closed periodic cubic spline about the sketch origin through ten
// poles at radii that wave round it, some 30 mm across from its middle.
g::Region spline_outline() {
  g::Segment s;
  s.name = "c1";
  s.kind = g::SegmentKind::BSpline;
  s.degree = 3;
  s.periodic = true;
  for (int i = 0; i < 10; ++i) {
    const double t = i * kPi / 5.0;
    const double rho = 30.0 * (1.0 + 0.2 * std::cos(2 * t) + 0.12 * std::sin(3 * t));
    s.poles.emplace_back(rho * std::cos(t), rho * std::sin(t));
  }
  for (int i = 0; i <= 10; ++i) {
    s.knots.push_back(i);
    s.multiplicities.push_back(1);
  }
  return region_of("r{c1}", {s});
}

g::LoftSection section(const g::Frame& frame, const g::Region& region) {
  g::LoftSection s;
  s.frame = frame;
  s.region = region;
  return s;
}

// A merged loft of the two regions, on XY and `height` above it.
g::LoftSpec loft_xy(const g::Region& bottom, const g::Region& top, double height) {
  g::Frame lower;
  g::Frame upper;
  upper.origin = gp_Pnt(0, 0, height);
  g::LoftSpec spec;
  spec.feature = "F3";
  spec.sections = {section(lower, bottom), section(upper, top)};
  spec.merge_tangent = true;
  return spec;
}

// A merged loft of the two regions on XZ and on a plane `distance` along +Y.
g::LoftSpec loft_xz(const g::Region& first, const g::Region& second, double distance) {
  g::Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  g::Frame plane = xz;
  plane.origin = gp_Pnt(0, distance, 0);
  g::LoftSpec spec;
  spec.feature = "F3";
  spec.sections = {section(xz, first), section(plane, second)};
  spec.merge_tangent = true;
  return spec;
}

struct Measures {
  double volume = 0.0;
  double area = 0.0;
  // The reference's error estimates (relative); -1 where it failed.
  double volume_error = 0.0;
  double area_error = 0.0;
};

// The reference: a copy of the shape with its faces split where their
// surfaces are less than C2 continuous, integrated by OCCT's adaptive
// Gauss-Kronrod rule to kReferenceEps (spans of B-spline faces apart).
Measures reference_measures(const TopoDS_Shape& shape) {
  ShapeUpgrade_ShapeDivideContinuity divide(BRepBuilderAPI_Copy(shape).Shape());
  divide.SetBoundaryCriterion(GeomAbs_C2);
  divide.SetPCurveCriterion(GeomAbs_C0);
  divide.SetSurfaceCriterion(GeomAbs_C2);
  // Perform says whether it split anything; unsplit, the result is the
  // copy as it was.
  divide.Perform();
  const TopoDS_Shape split = divide.Result().IsNull() ? shape : divide.Result();
  Measures result;
  GProp_GProps volume;
  result.volume_error = BRepGProp::VolumePropertiesGK(split, volume, kReferenceEps, false, true);
  GProp_GProps area;
  result.area_error = BRepGProp::SurfaceProperties(split, area, kReferenceEps);
  result.volume = volume.Mass();
  result.area = area.Mass();
  return result;
}

// The volume and area of a copy of the shape meshed to kDeflection (the
// copy, so that the body's own faces carry no triangulation).
Measures mesh_measures(const TopoDS_Shape& shape) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(shape).Shape();
  const BRepMesh_IncrementalMesh mesh(copy, kDeflection, false, kAngle, true);
  CHECK(mesh.IsDone());
  for (TopExp_Explorer it(copy, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    CHECK(!BRep_Tool::Triangulation(TopoDS::Face(it.Current()), location).IsNull());
  }
  GProp_GProps volume;
  BRepGProp::VolumeProperties(copy, volume, false, false, true);
  GProp_GProps area;
  BRepGProp::SurfaceProperties(copy, area, false, true);
  return {volume.Mass(), area.Mass()};
}

// The body's reported volume and area (the bodies query's and Physical
// Properties') against the reference within kAgreement and the mesh's
// within kMeshAgreement.
void check_measures(const std::string& what, const g::Shape& body) {
  CHECK(BRepCheck_Analyzer(body.occt()).IsValid());
  using Clock = std::chrono::steady_clock;
  const auto seconds = [](Clock::time_point from, Clock::time_point to) {
    return std::chrono::duration<double>(to - from).count();
  };
  const Clock::time_point start = Clock::now();
  const g::MassProperties reported = g::mass_properties(body);
  const mitcad::analysis::PhysicalProperties physical = mitcad::analysis::physical_properties(body.occt());
  GProp_GProps fixed;
  BRepGProp::VolumeProperties(body.occt(), fixed);
  const Clock::time_point measured = Clock::now();
  const Measures reference = reference_measures(body.occt());
  const Clock::time_point referenced = Clock::now();
  const Measures mesh = mesh_measures(body.occt());
  const Clock::time_point meshed = Clock::now();
  std::printf("%s: %d faces; measured in %.2f s, the reference in %.2f s, the mesh in %.2f s\n", what.c_str(),
              body.face_count(), seconds(start, measured), seconds(measured, referenced), seconds(referenced, meshed));
  std::printf("%s: volume %.6f, the reference %.6f (error estimate %.1e, %.2e off), the mesh's %.6f (%.2e off); "
              "OCCT's default rule %.6f (%.2e off)\n",
              what.c_str(), reported.volume, reference.volume, reference.volume_error,
              relative(reported.volume, reference.volume), mesh.volume, relative(reported.volume, mesh.volume),
              fixed.Mass(), relative(fixed.Mass(), reference.volume));
  std::printf("%s: area %.6f, the reference %.6f (error estimate %.1e, %.2e off), the mesh's %.6f (%.2e off)\n",
              what.c_str(), reported.area, reference.area, reference.area_error,
              relative(reported.area, reference.area), mesh.area, relative(reported.area, mesh.area));
  std::fflush(stdout);
  CHECK(reported.volume > 0.0);
  CHECK(reference.volume_error >= 0.0 && reference.volume_error <= kReferenceError);
  CHECK(reference.area_error >= 0.0 && reference.area_error <= kReferenceError);
  CHECK_NEAR_TOL(reported.volume, reference.volume, kAgreement);
  CHECK_NEAR_TOL(reported.area, reference.area, kAgreement);
  CHECK_NEAR_TOL(reported.volume, mesh.volume, kMeshAgreement);
  CHECK_NEAR_TOL(reported.area, mesh.area, kMeshAgreement);
  // Physical Properties and the mass measure as the bodies query does.
  CHECK_NEAR_TOL(physical.volume, reported.volume, 1e-9);
  CHECK_NEAR_TOL(physical.area, reported.area, 1e-9);
}

const char* continuity_name(GeomAbs_Shape shape) {
  switch (shape) {
  case GeomAbs_C0:
    return "C0";
  case GeomAbs_G1:
    return "G1";
  case GeomAbs_C1:
    return "C1";
  case GeomAbs_G2:
    return "G2";
  case GeomAbs_C2:
    return "C2";
  case GeomAbs_C3:
    return "C3";
  case GeomAbs_CN:
    return "CN";
  }
  return "?";
}

// The merged loft's side is one face on a periodic surface (the case of
// mitcad#2309), and its continuity is printed (C1 where lines and arcs
// joined).
void check_merged_side(const std::string& what, const g::Shape& body) {
  std::vector<TopoDS_Face> sides;
  for (int i = 0; i < body.face_count(); ++i) {
    for (const std::string& name : body.face_names(i)) {
      if (name.rfind("F3:side(", 0) == 0) {
        sides.push_back(body.face(i));
        break;
      }
    }
  }
  CHECK(sides.size() == 1);
  if (sides.size() != 1) {
    std::printf("%s: %zu side faces\n", what.c_str(), sides.size());
    return;
  }
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(sides.front());
  CHECK(!surface.IsNull());
  if (surface.IsNull()) {
    return;
  }
  const bool periodic = surface->IsUPeriodic() || surface->IsVPeriodic();
  std::printf("%s: one side face, its surface %s, continuity %s\n", what.c_str(),
              periodic ? "periodic" : "not periodic", continuity_name(surface->Continuity()));
  CHECK(periodic);
}

// The loft, and its 2 mm shells Inside and Outside (Sharp, the first
// section's end removed), each measured against the reference and a mesh.
void check_loft(const char* what, const g::LoftSpec& spec, bool outside) {
  g::ShapePtr body;
  try {
    body = g::loft(spec);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", what, error.what());
  }
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  check_merged_side(what, *body);
  check_measures(std::string(what) + ", the loft", *body);
  const std::string removed = g::face_name("F3", "start", spec.sections.front().region.name);
  for (const bool out : {false, true}) {
    if (out && !outside) {
      continue;
    }
    g::ShellSpec shell;
    shell.faces = {removed};
    shell.inside = out ? 0.0 : 2.0;
    shell.outside = out ? 2.0 : 0.0;
    const std::string name = std::string(what) + (out ? ", shelled 2 mm Outside" : ", shelled 2 mm Inside");
    g::ShapePtr result;
    try {
      result = g::shell("F4", *body, shell);
    } catch (const std::exception& error) {
      std::printf("%s: %s\n", name.c_str(), error.what());
    }
    CHECK(result != nullptr);
    if (result) {
      check_measures(name, *result);
    }
  }
}

void test_circle_slot_r50() {
  check_loft("R50 to 188 x 42, Merge", loft_xz(circle(50), slot(73, 21), 100), true);
}

void test_circle_slot_r30() {
  check_loft("R30 to 160 x 40, Merge", loft_xy(circle(30), slot(60, 20), 50), true);
}

void test_circle_spline() {
  check_loft("circle to spline outline, Merge", loft_xy(circle(25), spline_outline(), 60), true);
}

// The owner's loft (mitcad#2271): a circle of radius 110.9655561796864 on
// XZ, a slot 520 x 80 (centres 220 either side, radius 40) 100 along Y,
// shelled 2 mm Inside with the circle's end removed.
void test_circle_slot_owner() {
  check_loft("the owner's R111 to 520 x 80, Merge", loft_xz(circle(110.9655561796864), slot(220, 40), 100), false);
}

struct Part {
  const char* name;
  void (*run)();
};

const Part kParts[] = {
    {"circle_slot_r50", test_circle_slot_r50},
    {"circle_slot_r30", test_circle_slot_r30},
    {"circle_spline", test_circle_spline},
    {"circle_slot_owner", test_circle_slot_owner},
};

} // namespace
} // namespace test

int main(int argc, char** argv) {
  const std::string only = argc > 1 ? argv[1] : "";
  bool found = only.empty();
  for (const test::Part& part : test::kParts) {
    if (only.empty() || only == part.name) {
      found = true;
      test::guarded(part.name, part.run);
    }
  }
  if (!found) {
    std::fprintf(stderr, "usage: test_merged_loft_volumes [PART]; the parts:");
    for (const test::Part& part : test::kParts) {
      std::fprintf(stderr, " %s", part.name);
    }
    std::fprintf(stderr, "\n");
    return 2;
  }
  if (test::failures == 0) {
    std::puts("test_merged_loft_volumes: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
