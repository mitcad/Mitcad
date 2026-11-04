// SPDX-License-Identifier: MIT
// Section Analysis' clip of a cable gland body in the plane of its profile
// (mitcad#1230): a revolved body with a modelled internal thread at one end
// and an external one at the other, cut by a plane through its axis that
// is the profile's own plane (Distance 0), where the revolutions' seams,
// the axis and the threads' start sections lie on the cutting slab's face.
// The body was shown whole there and cut at 1 mm. The clip now tries again
// with a fuzzy value and then with the plane moved by 1e-6 mm; the result
// must be a valid half of the body with its caps.

#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/analysis/section.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kH = 0.8660254037844386; // H / P

// The basic profile's diameters of a size (ISO 68-1): major, minor, pitch.
ThreadSpec thread_on(const std::string& feature, const std::string& face, double d, double pitch) {
  ThreadSpec spec;
  spec.feature = feature;
  spec.faces = {face};
  spec.pitch = pitch;
  spec.major = d;
  spec.minor = d - 1.25 * kH * pitch;
  spec.pitch_diameter = d - 0.75 * kH * pitch;
  return spec;
}

// A full turn of the region about the Y axis, in the XY plane.
ShapePtr turned(const std::string& feature, Region region) {
  RevolveSpec spec;
  spec.feature = feature;
  spec.regions = {std::move(region)};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0));
  spec.angle1 = 2 * kPi;
  return revolve(spec);
}

ShapePtr cut(const ShapePtr& body, const ShapePtr& tool) {
  const BooleanResult result = boolean(BooleanOp::Cut, {body.get()}, *tool);
  CHECK(result.pieces.size() == 1);
  return result.pieces.at(0).shape;
}

// The gland body, all of it revolved about the Y axis from profiles in the
// XY plane: a tube 24 to 60 mm across and 45 long; a 42 mm bore 18 deep
// at y = 0 with an M45x3 internal thread (the bore wider than the thread's
// minor diameter: its teeth are added); the shank turned down to 30 mm
// from y = 27 with an M30x2 external thread.
ShapePtr gland() {
  const ShapePtr tube = turned("F2", rectangle(1, 12, 0, 18, 45));
  const ShapePtr bored = cut(tube, turned("F4", rectangle(1, 0, -1, 21, 19)));
  const ShapePtr stepped = cut(bored, turned("F6", rectangle(1, 15, 27, 16, 19)));
  const ShapePtr nut = modeled_thread(*stepped, thread_on("F7", side("F4", 1, 1), 45, 3));
  return modeled_thread(*nut, thread_on("F8", side("F6", 1, 3), 30, 2));
}

// The area of the shape's planar faces that lie in the plane z = at.
double area_in_plane(const TopoDS_Shape& shape, double at) {
  double area = 0.0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    if (std::abs(std::abs(plane.Axis().Direction().Z()) - 1.0) > 1e-9 ||
        std::abs(plane.Location().Z() - at) > 1e-6) {
      continue;
    }
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    area += props.Mass();
  }
  return area;
}

double volume_of(const TopoDS_Shape& shape) { return mitcad::analysis::physical_properties(shape).volume; }

void test_gland_in_its_profile_plane() {
  const ShapePtr body = gland();
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  CHECK(!body->find_faces("F7:thread(" + side("F4", 1, 1) + ")").empty());
  CHECK(!body->find_faces("F8:thread(" + side("F6", 1, 3) + ")").empty());
  const double whole = volume(*body);
  const std::size_t faces = static_cast<std::size_t>(body->face_count());
  // The half of the profile's section on each side of the axis without the
  // threads: the tube's 18 x 45 less the bore's 9 x 18 and the step's
  // 15 x 18. The threads take a few per cent of it.
  const double profile = 2 * (18 * 45 - 9 * 18 - 15 * 18);
  // Distance 0: the plane of the profiles (z = 0), the normal along +Z;
  // then 1 mm off it, which was cut before too.
  for (const double distance : {0.0, 1.0}) {
    const mitcad::analysis::Plane plane{{0, 0, distance}, {0, 0, 1}};
    mitcad::analysis::ClipReport report;
    const TopoDS_Shape half = mitcad::analysis::clip(body->occt(), plane, nullptr, &report);
    std::printf("gland clip at %g mm: attempt %d%s%s\n", distance, static_cast<int>(report.attempt),
                report.failed.empty() ? "" : ", after ", report.failed.c_str());
    CHECK(!half.IsNull() && BRepCheck_Analyzer(half).IsValid());
    // About half of it; the helices make the two halves differ a little at
    // the threads' ends, and 1 mm off the axis takes about 1 mm x the
    // profile's section's length more.
    if (distance == 0.0) {
      CHECK_NEAR_TOL(volume_of(half), whole / 2, 0.01);
    } else {
      CHECK(volume_of(half) > whole / 2 && volume_of(half) < whole / 2 + 2 * profile);
    }
    // The caps: the section of the body in the plane.
    const double caps = area_in_plane(half, distance);
    CHECK(caps > 0.85 * profile && caps < 1.05 * profile);
  }
  // The body is as it was.
  CHECK_NEAR_TOL(volume(*body), whole, 1e-9);
  CHECK(static_cast<std::size_t>(body->face_count()) == faces);
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
}

} // namespace

void section_clip_tests() {
  guarded("gland_in_its_profile_plane", test_gland_in_its_profile_plane);
}

} // namespace test
