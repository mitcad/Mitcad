// SPDX-License-Identifier: MIT
// Offset Face on the outer wall of a hollow loft from a slot to a circle
// (mitcad#2546, GitHub mitcad/Mitcad#723), through the geometry facade as
// the model builds the reported design: a slot 120 x 40 (arc centres 40
// either side, radius 20) 50 mm above a circle of radius 15, lofted from
// the slot to the circle with both ends leaving their sections square
// (Direction, angle 0, weight 1) and the sections' tangent edges merged, so
// the side is one periodic B-spline face, continuous only in its first
// derivative along the slot's former joins of lines and arcs; shelled 2 mm
// inside with both ends removed. The inner wall is the offset of that side,
// which the geometry kernel counts as only C0 continuous (an offset of a C1
// surface), and its offset of the body refused the whole body for it
// (BRepOffset_C0Geometry): moving the outer wall 2 mm out, Tangent Chain
// on, failed with "the neighbouring faces cannot follow the moved faces".
// The offset now splits such a face at its creases in its working copy
// (faceops.cpp, move_faces), as Shell does.
//
// The result is a valid single solid, open at both ends (two planar rims,
// at z = 0 and z = 50, and no cap); the outer wall lies 2 mm from where it
// was, the inner wall (in pieces) on its surface; the box grows by 2 mm
// round the slot's end; and the wall is the one the loft shelled 2 mm
// inside and 2 mm outside gives, by volume.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <ShapeUpgrade_ShapeDivideContinuity.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

constexpr double kDistance = 2.0; // the reported move, mm
constexpr double kWall = 2.0;     // the shell's wall, mm

g::Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

g::Segment arc(const std::string& name, double cx, double cy, double radius, double from, double to) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Arc;
  s.center = gp_Pnt2d(cx, cy);
  s.radius = radius;
  s.start_angle = from;
  s.end_angle = to;
  return s;
}

// The reported slot, its segments named as the model keys them,
// counter-clockwise from the upper straight side.
g::Region slot() {
  g::Region region;
  region.name = "r{c1[c2,c4],c2[c3,c1],c3[c4,c2],c4[c1,c3]}";
  region.loops.push_back(g::Loop{{line("c1[c2,c4]", 40, 20, -40, 20), arc("c4[c1,c3]", -40, 0, 20, kPi / 2, 3 * kPi / 2),
                                  line("c3[c4,c2]", -40, -20, 40, -20), arc("c2[c3,c1]", 40, 0, 20, -kPi / 2, kPi / 2)}});
  return region;
}

g::Region circle() {
  g::Segment s;
  s.name = "c2";
  s.kind = g::SegmentKind::Circle;
  s.center = gp_Pnt2d(0, 0);
  s.radius = 15.0;
  g::Region region;
  region.name = "r{c2}";
  region.loops.push_back(g::Loop{{s}});
  return region;
}

g::LoftEnd square() {
  g::LoftEnd end;
  end.kind = g::LoftEnd::Kind::Direction;
  end.angle = 0.0;
  end.weight = 1.0;
  return end;
}

// Loft1 (F4) from the slot 50 mm up to the circle on XY.
g::ShapePtr reported_loft() {
  g::LoftSpec spec;
  spec.feature = "F4";
  g::LoftSection top;
  top.frame.origin = gp_Pnt(0, 0, 50);
  top.region = slot();
  g::LoftSection bottom;
  bottom.region = circle();
  spec.sections = {top, bottom};
  spec.start = square();
  spec.end = square();
  spec.merge_tangent = true;
  return g::loft(spec);
}

bool starts_with(const std::string& text, const std::string& prefix) { return text.rfind(prefix, 0) == 0; }

// The faces holding a name that starts with `prefix`.
std::vector<int> faces_named(const g::Shape& body, const std::string& prefix) {
  std::vector<int> faces;
  for (int i = 0; i < body.face_count(); ++i) {
    const g::NameList& names = body.face_names(i);
    if (std::any_of(names.begin(), names.end(), [&](const std::string& n) { return starts_with(n, prefix); })) {
      faces.push_back(i);
    }
  }
  return faces;
}

// Points inside the face: its surface on a grid over its parameter box,
// short of its ends along v (the rims), all round along u.
std::vector<gp_Pnt> points_on(const TopoDS_Face& face) {
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface surface(face);
  std::vector<gp_Pnt> points;
  for (int i = 0; i < 12; ++i) {
    for (int j = 1; j < 6; ++j) {
      const double u = u0 + (u1 - u0) * (i + 0.5) / 12.0;
      const double v = v0 + (v1 - v0) * (0.1 + 0.8 * j / 6.0);
      points.push_back(surface.Value(u, v));
    }
  }
  return points;
}

double distance_to(const TopoDS_Shape& shape, const gp_Pnt& point) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), shape);
  return distance.IsDone() ? distance.Value() : -1.0;
}

// The volume on a copy split where the faces' surfaces are less than C2:
// the kernel's integration over a whole merged side and its offsets is far
// off (mitcad#2271).
double volume_of(const TopoDS_Shape& shape) {
  ShapeUpgrade_ShapeDivideContinuity divide(BRepBuilderAPI_Copy(shape).Shape());
  divide.SetBoundaryCriterion(GeomAbs_C2);
  divide.SetPCurveCriterion(GeomAbs_C0);
  divide.SetSurfaceCriterion(GeomAbs_C2);
  divide.Perform();
  const TopoDS_Shape split = divide.Result();
  GProp_GProps props;
  BRepGProp::VolumeProperties(split.IsNull() ? shape : split, props);
  return props.Mass();
}

int solids_of(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// Shell1 (F5): both ends removed, `inside` and `outside` thick.
g::ShapePtr shelled(const g::Shape& loft, double inside, double outside) {
  g::ShellSpec spec;
  for (const char* prefix : {"F4:start(", "F4:end("}) {
    for (int f : faces_named(loft, prefix)) {
      spec.faces.push_back(loft.face_names(f).front());
    }
  }
  CHECK(spec.faces.size() == 2);
  spec.inside = inside;
  spec.outside = outside;
  return g::shell("F5", loft, spec);
}

void test_outer_wall_moved_out() {
  const g::ShapePtr loft = reported_loft();
  CHECK(BRepCheck_Analyzer(loft->occt()).IsValid());
  const g::ShapePtr body = shelled(*loft, kWall, 0.0);
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  const std::vector<int> outer = faces_named(*body, "F4:side(");
  const std::vector<int> inner = faces_named(*body, "F5:offset(F4:side(");
  CHECK(outer.size() == 1);
  CHECK(inner.size() == 1);
  if (outer.size() != 1 || inner.size() != 1) {
    return;
  }
  const TopoDS_Face outer_before = body->face(outer.front());
  const TopoDS_Face inner_before = body->face(inner.front());
  // The inner wall, an offset of a C1 surface, is C0 as the kernel counts it.
  std::printf("the inner wall's surface is %s\n",
              BRep_Tool::Surface(inner_before)->Continuity() == GeomAbs_C0 ? "C0" : "C1 or smoother");

  // The outer wall 2 mm out, Tangent Chain on, as the report asks.
  const g::ShapePtr moved =
      g::offset_faces("F6", *body, {body->face_names(outer.front()).front()}, kDistance, true);
  CHECK(BRepCheck_Analyzer(moved->occt()).IsValid());
  CHECK(solids_of(moved->occt()) == 1);

  // Both ends open: the two rims, planar at z = 0 and z = 50, and no other
  // planar face.
  std::vector<double> rims;
  for (int i = 0; i < moved->face_count(); ++i) {
    const BRepAdaptor_Surface surface(moved->face(i));
    if (surface.GetType() == GeomAbs_Plane) {
      rims.push_back(surface.Plane().Location().Z());
      CHECK(surface.Plane().Axis().Direction().IsParallel(gp_Dir(0, 0, 1), 1.0e-9));
    }
  }
  CHECK(rims.size() == 2);
  std::sort(rims.begin(), rims.end());
  if (rims.size() == 2) {
    CHECK(std::abs(rims[0]) < 1.0e-7);
    CHECK(std::abs(rims[1] - 50.0) < 1.0e-7);
  }

  // The outer wall 2 mm from where it was; the inner wall, in pieces or
  // whole, on its surface.
  const std::vector<int> outer_after = faces_named(*moved, "F4:side(");
  const std::vector<int> inner_after = faces_named(*moved, "F5:offset(F4:side(");
  CHECK(!outer_after.empty());
  CHECK(!inner_after.empty());
  for (int f : outer_after) {
    for (const gp_Pnt& p : points_on(moved->face(f))) {
      const double d = distance_to(outer_before, p);
      if (std::abs(d - kDistance) > 1.0e-4) {
        std::fprintf(stderr, "outer wall point (%.4f %.4f %.4f) %.6f from the wall before\n", p.X(), p.Y(), p.Z(),
                     d);
      }
      CHECK(std::abs(d - kDistance) < 1.0e-4);
    }
  }
  for (int f : inner_after) {
    for (const gp_Pnt& p : points_on(moved->face(f))) {
      CHECK(distance_to(inner_before, p) < 1.0e-6);
    }
  }

  // 2 mm out round the slot's end, where the wall leaves the slot square.
  Bnd_Box box;
  BRepBndLib::AddOptimal(moved->occt(), box, false, false);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  std::printf("box [%.6f %.6f %.6f]-[%.6f %.6f %.6f]\n", x0, y0, z0, x1, y1, z1);
  CHECK(std::abs(x0 + 62.0) < 1.0e-3 && std::abs(x1 - 62.0) < 1.0e-3);
  CHECK(std::abs(y0 + 22.0) < 1.0e-3 && std::abs(y1 - 22.0) < 1.0e-3);
  CHECK(std::abs(z0) < 1.0e-3 && std::abs(z1 - 50.0) < 1.0e-3);

  // The wall from 2 mm inside the loft's side to 2 mm outside it: the loft
  // shelled 2 mm inside and 2 mm outside, both ends removed.
  const g::ShapePtr both = shelled(*loft, kWall, kDistance);
  CHECK(BRepCheck_Analyzer(both->occt()).IsValid());
  const double volume = volume_of(moved->occt());
  const double expected = volume_of(both->occt());
  std::printf("volume %.6f, the loft shelled both ways %.6f, before %.6f\n", volume, expected,
              volume_of(body->occt()));
  CHECK_NEAR_TOL(volume, expected, 1.0e-5);
  CHECK(volume > volume_of(body->occt()));
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(offset_face_loft_slot_circle) {
  test::guarded("outer wall moved out", test::test_outer_wall_moved_out);
  return test::failures == 0 ? 0 : 1;
}
