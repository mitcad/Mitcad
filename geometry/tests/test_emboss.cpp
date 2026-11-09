// SPDX-License-Identifier: MIT
// Emboss: profiles wrapped onto cylinders (mitcad#458). Volumes are
// analytic: a region of area A in the sketch, wrapped onto the cylinder of
// radius r and grown radially to R, is A |R^2 - r^2| / (2 r). Profiles
// projected onto a sphere, a B-spline face and a plane (mitcad#1091).
// Tapers on a cylinder, a cone, a plane and a sphere (mitcad#1547).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <utility>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "../src/emboss_projection.hpp"
#include "check.hpp"
#include "mitcad/geometry/emboss.hpp"
#include "mitcad/geometry/text.hpp"

namespace {

using namespace mitcad::geometry;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

int solids(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_SOLID); e.More(); e.Next()) {
    ++n;
  }
  return n;
}

// A rectangle with named sides c1..c4 and the region name r1.
Region rectangle(double x, double y, double width, double height, const std::string& name = "r1") {
  Region region = rectangle_region(x, y, width, height);
  region.name = name;
  int i = 0;
  for (Segment& s : region.loops.front().segments) {
    s.name = name + "c" + std::to_string(++i);
  }
  return region;
}

double wrapped_volume(double area, double r, double to) { return area * std::abs(to * to - r * r) / (2.0 * r); }

// The sketch plane tangent to the cylinder of radius 10 about Z at +Y, its
// x along -X (so that s runs along it) and y along Z.
EmbossWrapSpec spec_on_y(const std::vector<Region>& regions, double to) {
  EmbossWrapSpec spec;
  spec.feature = "F5";
  spec.frame.origin = gp_Pnt(0.0, 10.0, 0.0);
  spec.frame.x_axis = gp_Dir(-1.0, 0.0, 0.0);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.regions = regions;
  spec.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
  spec.radius = 10.0;
  spec.to_radius = to;
  return spec;
}

void test_a_rectangle_raised_and_sunk() {
  for (const double to : {11.0, 9.0}) {
    const ShapePtr tool = wrapped_emboss(spec_on_y({rectangle(-2.0, 5.0, 4.0, 5.0)}, to));
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 1);
    CHECK_NEAR(volume_of(tool->occt()), wrapped_volume(20.0, 10.0, to));
    CHECK_NEAR(mass_properties(*tool).volume, wrapped_volume(20.0, 10.0, to));
    CHECK(tool->find_faces("F5:start(r1)").size() == 1);
    CHECK(tool->find_faces("F5:end(r1)").size() == 1);
    for (int i = 1; i <= 4; ++i) {
      CHECK(tool->find_faces("F5:side(r1c" + std::to_string(i) + ")").size() == 1);
    }
    // Joined to or cut from the rod (radius 10, 20 long): its volume grows
    // or shrinks by the tool's.
    const TopoDS_Shape rod = BRepPrimAPI_MakeCylinder(10.0, 20.0).Shape();
    const double before = volume_of(rod);
    if (to > 10.0) {
      BRepAlgoAPI_Fuse fuse(rod, tool->occt());
      CHECK(fuse.IsDone() && BRepCheck_Analyzer(fuse.Shape()).IsValid());
      CHECK_NEAR(volume_of(fuse.Shape()) - before, 21.0);
    } else {
      BRepAlgoAPI_Cut cut(rod, tool->occt());
      CHECK(cut.IsDone() && BRepCheck_Analyzer(cut.Shape()).IsValid());
      CHECK_NEAR(before - volume_of(cut.Shape()), 19.0);
    }
    // The tool lies about +Y (s = 0 under the sketch's middle), within the
    // profile's heights.
    Bnd_Box box;
    BRepBndLib::Add(tool->occt(), box);
    double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
    box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    CHECK(std::abs(xmin + xmax) < 1e-6 && ymin > 8.5 && zmin > 4.9 && zmax < 10.1);
  }
}

void test_curves_holes_and_a_rotated_axis() {
  // A circle (a B-spline on the cylinder) and a rectangle with a hole, on
  // a cylinder of radius 10 about X through (123, -456, 789).
  const gp_Pnt origin(123.0, -456.0, 789.0);
  Region holed = rectangle(-2.0, 5.0, 4.0, 5.0, "r2");
  Region hole = rectangle(-1.0, 6.0, 2.0, 3.0, "h");
  holed.loops.push_back(hole.loops.front());
  Region circle = circle_region(6.0, 7.5, 1.5);
  circle.name = "r3";
  circle.loops.front().segments.front().name = "c9";
  for (const double to : {11.0, 9.0}) {
    EmbossWrapSpec spec;
    spec.feature = "F7";
    spec.frame.origin = origin.Translated(gp_Vec(0.0, 10.0, 0.0));
    spec.frame.x_axis = gp_Dir(0.0, 0.0, 1.0);
    spec.frame.y_axis = gp_Dir(1.0, 0.0, 0.0);
    spec.regions = {holed, circle};
    spec.axis = gp_Ax1(origin, gp_Dir(1.0, 0.0, 0.0));
    spec.radius = 10.0;
    spec.to_radius = to;
    const ShapePtr tool = wrapped_emboss(spec);
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 2);
    const double area = 20.0 - 6.0 + test::kPi * 1.5 * 1.5;
    CHECK_NEAR_TOL(mass_properties(*tool).volume, wrapped_volume(area, 10.0, to), 1e-6);
    CHECK(tool->find_faces("F7:side(c9)").size() == 1);
    CHECK(tool->find_faces("F7:end(r3)").size() == 1);
    CHECK(tool->find_faces("F7:side(hc1)").size() == 1);
  }
}

void test_a_bore_and_a_sketch_off_the_cylinder() {
  // Raised into a bore of radius 10 (towards the axis) from a sketch plane
  // 30 away from it: lengths are the cylinder's, wherever the sketch lies.
  EmbossWrapSpec spec = spec_on_y({rectangle(-2.0, 5.0, 4.0, 5.0)}, 9.5);
  spec.frame.origin = gp_Pnt(0.0, 30.0, 0.0);
  const ShapePtr tool = wrapped_emboss(spec);
  CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
  CHECK_NEAR(mass_properties(*tool).volume, wrapped_volume(20.0, 10.0, 9.5));
  // Every point of the tool lies on the +Y side.
  Bnd_Box box;
  BRepBndLib::Add(tool->occt(), box);
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  CHECK(ymin > 9.0);
  CHECK(zmin > 4.9 && zmax < 10.1);
}

// The glyph outlines of a text (the bundled font, `height` mm high from a
// baseline at y) as regions, one per letter, its counters as holes, named
// r_<letter>; `left` the letters' flat extents along x.
std::vector<Region> text_regions(const std::string& text, double height, double x, double y,
                                 std::vector<std::pair<double, double>>& extents) {
  const TextOutlines outlines = text_outlines("", false, false, text);
  std::vector<Region> regions;
  double pen = 0.0;
  for (std::size_t k = 0; k < outlines.glyphs.size(); ++k) {
    const GlyphOutline& glyph = outlines.glyphs[k];
    std::vector<Loop> loops;
    std::vector<double> areas;
    std::vector<std::vector<gp_Pnt2d>> samples;
    double low = 1e300;
    double high = -1e300;
    int piece_number = 0;
    for (const auto& contour : glyph.contours) {
      Loop loop;
      std::vector<gp_Pnt2d> points;
      for (const auto& piece : contour) {
        std::vector<gp_Pnt2d> placed;
        for (const gp_Pnt2d& p : piece) {
          placed.emplace_back(x + (pen + p.X()) * height, y + p.Y() * height);
        }
        Segment s;
        s.name = text.substr(k, 1) + std::to_string(++piece_number);
        if (placed.size() == 2) {
          s.kind = SegmentKind::Line;
          s.start = placed[0];
          s.end = placed[1];
        } else {
          s.kind = SegmentKind::BSpline;
          s.degree = static_cast<int>(placed.size()) - 1;
          s.poles = placed;
          s.knots = {0.0, 1.0};
          s.multiplicities = {s.degree + 1, s.degree + 1};
        }
        loop.segments.push_back(s);
        // Points along the piece (de Casteljau) for its extent and area.
        for (int i = 0; i < 64; ++i) {
          std::vector<gp_Pnt2d> q = placed;
          const double t = i / 64.0;
          for (std::size_t n = q.size() - 1; n > 0; --n) {
            for (std::size_t j = 0; j < n; ++j) {
              q[j] = gp_Pnt2d(q[j].X() + t * (q[j + 1].X() - q[j].X()), q[j].Y() + t * (q[j + 1].Y() - q[j].Y()));
            }
          }
          points.push_back(q[0]);
          low = std::min(low, q[0].X());
          high = std::max(high, q[0].X());
        }
      }
      double area = 0.0;
      for (std::size_t i = 0; i < points.size(); ++i) {
        const gp_Pnt2d& p = points[i];
        const gp_Pnt2d& q = points[(i + 1) % points.size()];
        area += p.X() * q.Y() - q.X() * p.Y();
      }
      loops.push_back(loop);
      areas.push_back(area / 2.0);
      samples.push_back(points);
    }
    pen += glyph.advance;
    if (loops.empty()) {
      continue;
    }
    // The largest contour bounds the letter; the others are its counters.
    std::size_t outer = 0;
    for (std::size_t i = 1; i < areas.size(); ++i) {
      if (std::abs(areas[i]) > std::abs(areas[outer])) {
        outer = i;
      }
    }
    Region region;
    region.name = "r_" + text.substr(k, 1);
    region.loops.push_back(loops[outer]);
    for (std::size_t i = 0; i < loops.size(); ++i) {
      if (i != outer) {
        region.loops.push_back(loops[i]);
      }
    }
    regions.push_back(region);
    extents.emplace_back(low, high);
  }
  return regions;
}

// mitcad#473: "ABC" wrapped onto a cylinder 30 mm across and raised 1 mm.
// The letters' outer faces lie on the cylinder of radius 16, and each
// letter spans the angle of its flat width over the radius 15.
void test_text_wrapped_on_a_cylinder() {
  std::vector<std::pair<double, double>> extents;
  const std::vector<Region> regions = text_regions("ABC", 10.0, -10.0, 5.0, extents);
  CHECK(regions.size() == 3);
  EmbossWrapSpec spec;
  spec.feature = "F5";
  spec.frame.origin = gp_Pnt(0.0, 15.0, 0.0);
  spec.frame.x_axis = gp_Dir(-1.0, 0.0, 0.0);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.regions = regions;
  spec.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
  spec.radius = 15.0;
  spec.to_radius = 16.0;
  const ShapePtr tool = wrapped_emboss(spec);
  CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
  CHECK(solids(tool->occt()) == 3);
  for (std::size_t k = 0; k < regions.size(); ++k) {
    const std::string& name = regions[k].name;
    const std::vector<int> tops = tool->find_faces("F5:end(" + name + ")");
    CHECK(tops.size() == 1);
    if (tops.size() != 1) {
      continue;
    }
    const TopoDS_Face top = tool->face(tops.front());
    const BRepAdaptor_Surface surface(top);
    CHECK(surface.GetType() == GeomAbs_Cylinder);
    // Points of the face's edges: on the cylinder of radius 16 within
    // 1e-4 mm, and their angles from +Y (s along -X) span the letter.
    double low = 1e300;
    double high = -1e300;
    double worst = 0.0;
    for (TopExp_Explorer e(top, TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
      for (int i = 0; i <= 256; ++i) {
        const gp_Pnt p = curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 256.0);
        worst = std::max(worst, std::abs(std::hypot(p.X(), p.Y()) - 16.0));
        const double angle = std::atan2(-p.X(), p.Y());
        low = std::min(low, angle);
        high = std::max(high, angle);
      }
    }
    CHECK(worst < 1e-4);
    const double flat = (extents[k].second - extents[k].first) / 15.0;
    if (std::abs((high - low) - flat) > 1e-4) {
      std::printf("letter %s: angular width %.7f, flat width over the radius %.7f\n", name.c_str(), high - low, flat);
    }
    CHECK(std::abs((high - low) - flat) < 1e-4);
    // And where: the letter's middle at its flat middle's angle.
    const double middle = (extents[k].first + extents[k].second) / 2.0 / 15.0;
    CHECK(std::abs((high + low) / 2.0 - middle) < 1e-4);
  }
}

// mitcad#473: a rectangle developed onto a cone and raised along its
// normals. The cone of half angle 30 degrees about Z, its apex at the
// origin, opening upwards; the sketch beside it at the height 20 (radius
// 20 tan 30). Raised by 1: every point of the top face lies 1 off the
// cone; the face's area is the rectangle's scaled by the offset cone's
// spread at that distance, and the volume about the rectangle's area by
// the depth.
void test_a_rectangle_developed_on_a_cone() {
  const double angle = test::kPi / 6.0;
  const double r0 = 20.0 * std::tan(angle);
  EmbossWrapSpec spec;
  spec.feature = "F6";
  spec.frame.origin = gp_Pnt(0.0, r0, 20.0);
  spec.frame.x_axis = gp_Dir(-1.0, 0.0, 0.0);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.regions = {rectangle(-3.0, -2.0, 6.0, 4.0)};
  spec.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
  spec.radius = r0;
  spec.to_radius = r0 + 1.0;
  spec.half_angle = angle;
  for (const double offset : {1.0, -1.0}) {
    spec.to_radius = r0 + offset;
    const ShapePtr tool = wrapped_emboss(spec);
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 1);
    CHECK(tool->find_faces("F6:end(r1)").size() == 1);
    CHECK(tool->find_faces("F6:side(r1c1)").size() == 1);
    // The bottom face lies on the cone: area 24 (a development keeps
    // areas), the volume about 24 by the depth.
    const std::vector<int> bottom = tool->find_faces("F6:start(r1)");
    CHECK(bottom.size() == 1);
    if (!bottom.empty()) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(tool->face(bottom.front()), props);
      CHECK_NEAR_TOL(props.Mass(), 24.0, 1e-6);
    }
    const double volume = volume_of(tool->occt());
    CHECK(std::abs(volume - 24.0) < 24.0 * 0.15);
    // Points of the top face's edges: 1 from the cone.
    const std::vector<int> top = tool->find_faces("F6:end(r1)");
    double worst = 0.0;
    if (!top.empty()) {
      for (TopExp_Explorer e(tool->face(top.front()), TopAbs_EDGE); e.More(); e.Next()) {
        const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
        for (int i = 0; i <= 64; ++i) {
          const gp_Pnt p = curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 64.0);
          // Distance from the cone z = r / tan(angle): (r cos - z sin).
          const double distance = std::hypot(p.X(), p.Y()) * std::cos(angle) - p.Z() * std::sin(angle);
          worst = std::max(worst, std::abs(distance - offset));
        }
      }
    }
    CHECK(worst < 1e-6);
  }
  EmbossWrapSpec past = spec;
  past.regions = {rectangle(-3.0, -60.0, 6.0, 4.0)};
  past.to_radius = r0 + 1.0;
  CHECK(test::throws_with([&] { wrapped_emboss(past); }, "apex"));
}

void test_refusals() {
  EmbossWrapSpec across = spec_on_y({rectangle(-2.0, 5.0, 4.0, 5.0)}, 11.0);
  across.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 1.0, 0.0));
  CHECK(test::throws_with([&] { wrapped_emboss(across); }, "parallel to its axis"));
  const EmbossWrapSpec wide = spec_on_y({rectangle(-40.0, 5.0, 80.0, 5.0)}, 11.0);
  CHECK(test::throws_with([&] { wrapped_emboss(wide); }, "wider than its circumference"));
  const EmbossWrapSpec flat = spec_on_y({rectangle(-2.0, 5.0, 4.0, 5.0)}, 10.0);
  CHECK(test::throws_with([&] { wrapped_emboss(flat); }, "no depth"));
}

// --- Projection onto any face (mitcad#1091)

// Points inside a face: a grid over its parameters, those inside it.
std::vector<gp_Pnt> inner_points(const TopoDS_Face& face) {
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  BRepTopAdaptor_FClass2d inside(face, Precision::PConfusion());
  const BRepAdaptor_Surface surface(face);
  std::vector<gp_Pnt> points;
  constexpr int kGrid = 12;
  for (int i = 0; i <= kGrid; ++i) {
    for (int j = 0; j <= kGrid; ++j) {
      const gp_Pnt2d uv(u0 + (u1 - u0) * i / kGrid, v0 + (v1 - v0) * j / kGrid);
      if (inside.Perform(uv) == TopAbs_IN) {
        points.push_back(surface.Value(uv.X(), uv.Y()));
      }
    }
  }
  return points;
}

// The one face of a tool a name gives.
TopoDS_Face face_named(const Shape& tool, const std::string& name) {
  const std::vector<int> found = tool.find_faces(name);
  CHECK(found.size() == 1);
  return found.empty() ? TopoDS_Face() : tool.face(found.front());
}

double area_of(const TopoDS_Face& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

// A ball of radius 20 about the origin (the sphere's poles on Z, its seam
// on +X), its face named F1:side0.
Shape ball() {
  const TopoDS_Shape solid = BRepPrimAPI_MakeSphere(gp_Pnt(0.0, 0.0, 0.0), 20.0).Shape();
  const TopExp_Explorer face(solid, TopAbs_FACE);
  return Shape(solid, {{face.Current(), {"F1:side0"}}});
}

// The spec of a sketch on XY (normal +Z) projected along +Z onto `face`.
EmbossProjectSpec projection_on_xy(const std::vector<Region>& regions, const std::string& face, double depth) {
  EmbossProjectSpec spec;
  spec.feature = "F3";
  spec.regions = regions;
  spec.face = face;
  spec.direction = gp_Vec(0.0, 0.0, 1.0);
  spec.depth = depth;
  return spec;
}

// A square projected onto the ball along +Z from the XY plane through its
// centre (the upper half, the nearest ahead) and raised or sunk 1 along the
// sphere's normals: the top lies on the sphere of radius 21 (or 19), the
// bottom on the ball over the square, and the walls along the radii, so
// the volume is the bottom's solid angle (its area / 20^2) times
// |R^3 - 20^3| / 3. Off the pole and the seam, and over the pole.
void test_a_square_projected_onto_a_sphere() {
  const Shape body = ball();
  for (const double depth : {1.0, -1.0}) {
    const ShapePtr tool = projected_emboss(body, projection_on_xy({rectangle(3.0, 3.0, 6.0, 6.0)}, "F1:side0", depth));
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 1);
    for (int i = 1; i <= 4; ++i) {
      CHECK(tool->find_faces("F3:side(r1c" + std::to_string(i) + ")").size() == 1);
    }
    const TopoDS_Face bottom = face_named(*tool, "F3:start(r1)");
    const TopoDS_Face top = face_named(*tool, "F3:end(r1)");
    const std::vector<gp_Pnt> on_ball = inner_points(bottom);
    const std::vector<gp_Pnt> over = inner_points(top);
    CHECK(on_ball.size() > 10 && over.size() > 10);
    double worst = 0.0;
    for (const gp_Pnt& p : on_ball) {
      worst = std::max(worst, std::abs(p.Distance(gp::Origin()) - 20.0));
      CHECK(p.Z() > 0.0 && p.X() > 3.0 - 1e-6 && p.X() < 9.0 + 1e-6 && p.Y() > 3.0 - 1e-6 && p.Y() < 9.0 + 1e-6);
    }
    for (const gp_Pnt& p : over) {
      worst = std::max(worst, std::abs(p.Distance(gp::Origin()) - (20.0 + depth)));
    }
    CHECK(worst < 1e-6);
    const double outer = depth > 0.0 ? 21.0 : 20.0;
    const double inner = depth > 0.0 ? 20.0 : 19.0;
    const double shell = area_of(bottom) / 400.0 * (std::pow(outer, 3) - std::pow(inner, 3)) / 3.0;
    CHECK_NEAR_TOL(volume_of(tool->occt()), shell, 1e-5);
    // Joined to the ball (raised) or cut from it (sunk).
    const double before = volume_of(body.occt());
    if (depth > 0.0) {
      BRepAlgoAPI_Fuse fuse(body.occt(), tool->occt());
      CHECK(fuse.IsDone() && BRepCheck_Analyzer(fuse.Shape()).IsValid());
      CHECK_NEAR_TOL(volume_of(fuse.Shape()) - before, shell, 1e-5);
    } else {
      BRepAlgoAPI_Cut cut(body.occt(), tool->occt());
      CHECK(cut.IsDone() && BRepCheck_Analyzer(cut.Shape()).IsValid());
      CHECK_NEAR_TOL(before - volume_of(cut.Shape()), shell, 1e-5);
    }
  }
  // Over the pole (the seam runs through the square: walls may come in
  // pieces).
  const ShapePtr pole = projected_emboss(body, projection_on_xy({rectangle(-5.0, -5.0, 10.0, 10.0)}, "F1:side0", 1.0));
  CHECK(BRepCheck_Analyzer(pole->occt()).IsValid());
  CHECK(solids(pole->occt()) == 1);
  CHECK(!pole->find_faces("F3:side(r1c2)").empty());
  double worst = 0.0;
  for (const gp_Pnt& p : inner_points(face_named(*pole, "F3:end(r1)"))) {
    worst = std::max(worst, std::abs(p.Distance(gp::Origin()) - 21.0));
  }
  CHECK(worst < 1e-6);
  // Bounds: the top reaches the pole of the sphere of radius 21.
  Bnd_Box box;
  BRepBndLib::AddOptimal(pole->occt(), box, false, false);
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  CHECK(std::abs(zmax - 21.0) < 1e-5);
}

// A bump: a B-spline face over [-20, 20] x [-20, 20], 6 high in the middle,
// its normal up; named F1:bump.
occ::handle<Geom_BSplineSurface> bump_surface() {
  NCollection_Array2<gp_Pnt> poles(1, 4, 1, 4);
  for (int i = 1; i <= 4; ++i) {
    for (int j = 1; j <= 4; ++j) {
      const bool middle = (i == 2 || i == 3) && (j == 2 || j == 3);
      poles(i, j) = gp_Pnt(-20.0 + 40.0 * (i - 1) / 3.0, -20.0 + 40.0 * (j - 1) / 3.0, middle ? 6.0 : 0.0);
    }
  }
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  NCollection_Array1<int> multiplicities(1, 2);
  multiplicities(1) = 4;
  multiplicities(2) = 4;
  return new Geom_BSplineSurface(poles, knots, knots, multiplicities, multiplicities, 3, 3);
}

Shape bump() {
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(bump_surface(), Precision::Confusion()).Face();
  return Shape(face, {{face, {"F1:bump"}}});
}

// A square projected onto the bump along +Z and raised 1 along its
// normals: every point of the top lies 1 from the bump, the bottom on it
// over the square.
void test_a_square_projected_onto_a_bspline_face() {
  const Shape body = bump();
  const occ::handle<Geom_BSplineSurface> surface = bump_surface();
  const ShapePtr tool = projected_emboss(body, projection_on_xy({rectangle(-3.0, -3.0, 6.0, 6.0)}, "F1:bump", 1.0));
  CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
  CHECK(solids(tool->occt()) == 1);
  for (int i = 1; i <= 4; ++i) {
    CHECK(tool->find_faces("F3:side(r1c" + std::to_string(i) + ")").size() == 1);
  }
  const std::vector<gp_Pnt> on_bump = inner_points(face_named(*tool, "F3:start(r1)"));
  const std::vector<gp_Pnt> over = inner_points(face_named(*tool, "F3:end(r1)"));
  CHECK(on_bump.size() > 10 && over.size() > 10);
  double worst = 0.0;
  for (const gp_Pnt& p : on_bump) {
    GeomAPI_ProjectPointOnSurf projection(p, surface);
    CHECK(projection.NbPoints() > 0);
    if (projection.NbPoints() > 0) {
      worst = std::max(worst, projection.LowerDistance());
    }
    CHECK(p.X() > -3.0 - 1e-6 && p.X() < 3.0 + 1e-6 && p.Y() > -3.0 - 1e-6 && p.Y() < 3.0 + 1e-6);
  }
  for (const gp_Pnt& p : over) {
    GeomAPI_ProjectPointOnSurf projection(p, surface);
    CHECK(projection.NbPoints() > 0);
    if (projection.NbPoints() > 0) {
      worst = std::max(worst, std::abs(projection.LowerDistance() - 1.0));
    }
    // Above the bump.
    CHECK(p.Z() > 1.0);
  }
  CHECK(worst < 1e-6);
  // The volume about the bottom's area by the depth.
  const double bottom = area_of(face_named(*tool, "F3:start(r1)"));
  CHECK(std::abs(volume_of(tool->occt()) - bottom) < 0.05 * bottom);
}

// Along a direction off the sketch normal: a 4 x 4 square at the origin of
// XY projected along (1, 0, 1) onto the top of a box 10 high lands 10
// further along X; raised 1 along the top's normal (straight up).
void test_a_direction_shifts_the_projection() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-20.0, -20.0, 0.0), 40.0, 40.0, 10.0).Shape();
  TopoDS_Face top;
  for (TopExp_Explorer f(box, TopAbs_FACE); f.More(); f.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f.Current(), props);
    if (std::abs(props.CentreOfMass().Z() - 10.0) < 1e-9) {
      top = TopoDS::Face(f.Current());
    }
  }
  CHECK(!top.IsNull());
  const Shape body(box, {{top, {"F1:top"}}});
  EmbossProjectSpec spec = projection_on_xy({rectangle(0.0, 0.0, 4.0, 4.0)}, "F1:top", 1.0);
  for (const bool slanted : {false, true}) {
    spec.direction = slanted ? gp_Vec(1.0, 0.0, 1.0) : gp_Vec(0.0, 0.0, 1.0);
    const ShapePtr tool = projected_emboss(body, spec);
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK_NEAR(volume_of(tool->occt()), 16.0);
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face_named(*tool, "F3:start(r1)"), props);
    CHECK_NEAR(props.Mass(), 16.0);
    const gp_Pnt centre = props.CentreOfMass();
    const double x = slanted ? 12.0 : 2.0;
    CHECK(std::abs(centre.X() - x) < 1e-6 && std::abs(centre.Y() - 2.0) < 1e-6 && std::abs(centre.Z() - 10.0) < 1e-6);
    Bnd_Box bounds;
    BRepBndLib::AddOptimal(tool->occt(), bounds, false, false);
    double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
    bounds.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    CHECK(std::abs(xmin - (x - 2.0)) < 1e-6 && std::abs(xmax - (x + 2.0)) < 1e-6);
    CHECK(std::abs(zmin - 10.0) < 1e-6 && std::abs(zmax - 11.0) < 1e-6);
  }
}

// mitcad#1154: along a slanted direction "nearest ahead of the sketch
// plane" is measured along the direction's lines. A 6 x 6 square on XY
// about (-45, 0) projected along (1, 0, 1) onto a ball of radius 20 about
// (-30, 0, 30): the line from the square's middle meets the ball's
// underside at z = 10.5 and its top at z = 34.5, both ahead of the plane,
// so the emboss stands on the underside, below the ball's centre (a
// measure along the direction alone put the underside behind the plane
// and the top ahead, and built it on the top). Raised 1 along the
// sphere's normals, the walls along its radii: the volume is the bottom's
// solid angle times (21^3 - 20^3) / 3.
void test_a_slanted_direction_takes_the_near_side_of_a_ball() {
  const gp_Pnt centre(-30.0, 0.0, 30.0);
  const TopoDS_Shape solid = BRepPrimAPI_MakeSphere(centre, 20.0).Shape();
  const TopExp_Explorer ball_face(solid, TopAbs_FACE);
  const Shape body(solid, {{ball_face.Current(), {"F1:side0"}}});
  EmbossProjectSpec spec = projection_on_xy({rectangle(-48.0, -3.0, 6.0, 6.0)}, "F1:side0", 1.0);
  spec.direction = gp_Vec(1.0, 0.0, 1.0);
  const ShapePtr tool = projected_emboss(body, spec);
  CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
  CHECK(solids(tool->occt()) == 1);
  const TopoDS_Face bottom = face_named(*tool, "F3:start(r1)");
  GProp_GProps props;
  BRepGProp::SurfaceProperties(bottom, props);
  CHECK(props.CentreOfMass().Z() < 30.0);
  // Every point of the bottom on the ball, on its underside.
  const std::vector<gp_Pnt> on_ball = inner_points(bottom);
  CHECK(on_ball.size() > 10);
  double worst = 0.0;
  for (const gp_Pnt& p : on_ball) {
    worst = std::max(worst, std::abs(p.Distance(centre) - 20.0));
    CHECK(p.Z() < 30.0);
  }
  CHECK(worst < 1e-6);
  for (int i = 1; i <= 4; ++i) {
    CHECK(!tool->find_faces("F3:side(r1c" + std::to_string(i) + ")").empty());
  }
  const double shell = area_of(bottom) / 400.0 * (std::pow(21.0, 3) - std::pow(20.0, 3)) / 3.0;
  CHECK_NEAR_TOL(volume_of(tool->occt()), shell, 1e-5);
  Bnd_Box bounds;
  BRepBndLib::AddOptimal(tool->occt(), bounds, false, false);
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  bounds.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  CHECK(zmax < 30.0);
}

// mitcad#1154: a point inside a face whose inside the grid over its
// parameters misses: a strip 0.01 wide along two edges of a 10 x 10 square
// (an L), every grid point at least 0.5 from its edges. The point lies in
// the strip, inside the face, and the normal is the plane's.
void test_a_point_inside_a_sliver() {
  BRepBuilderAPI_MakePolygon polygon;
  polygon.Add(gp_Pnt(0.0, 0.0, 0.0));
  polygon.Add(gp_Pnt(10.0, 0.0, 0.0));
  polygon.Add(gp_Pnt(10.0, 0.01, 0.0));
  polygon.Add(gp_Pnt(0.01, 0.01, 0.0));
  polygon.Add(gp_Pnt(0.01, 10.0, 0.0));
  polygon.Add(gp_Pnt(0.0, 10.0, 0.0));
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(gp_Pln(gp::XOY()), polygon.Wire(), true).Face();
  gp_Pnt point;
  gp_Vec normal;
  CHECK(detail::inner_point(face, point, normal));
  CHECK(std::abs(point.Z()) < 1e-6);
  const bool in_strip = point.X() > -1e-6 && point.Y() > -1e-6 && point.X() < 10.0 + 1e-6 && point.Y() < 10.0 + 1e-6 &&
                        (point.X() < 0.01 + 1e-6 || point.Y() < 0.01 + 1e-6);
  CHECK(in_strip);
  // The plane's parameters are its x and y.
  BRepTopAdaptor_FClass2d inside(face, Precision::PConfusion());
  CHECK(inside.Perform(gp_Pnt2d(point.X(), point.Y())) == TopAbs_IN);
  CHECK(std::abs(std::abs(normal.Z()) - 1.0) < 1e-6);
}

// Profiles that fall partly off the face, or past where it turns away from
// the direction, or miss it, are refused with their region named; the
// direction must cross the sketch plane.
void test_projection_refusals() {
  const Shape sphere = ball();
  CHECK(test::throws_with(
      [&] { projected_emboss(sphere, projection_on_xy({rectangle(15.0, -5.0, 10.0, 10.0)}, "F1:side0", 1.0)); },
      "the profile r1 falls partly off face F1:side0"));
  CHECK(test::throws_with(
      [&] { projected_emboss(sphere, projection_on_xy({rectangle(30.0, 30.0, 6.0, 6.0)}, "F1:side0", 1.0)); },
      "the profile r1 misses face F1:side0"));
  const Shape surface = bump();
  CHECK(test::throws_with(
      [&] { projected_emboss(surface, projection_on_xy({rectangle(17.0, -3.0, 6.0, 6.0)}, "F1:bump", 1.0)); },
      "the profile r1 falls partly off face F1:bump"));
  EmbossProjectSpec sideways = projection_on_xy({rectangle(3.0, 3.0, 6.0, 6.0)}, "F1:side0", 1.0);
  sideways.direction = gp_Vec(1.0, 0.0, 0.0);
  CHECK(test::throws_with([&] { projected_emboss(sphere, sideways); }, "lies in the sketch plane"));
  CHECK(test::throws_with(
      [&] { projected_emboss(sphere, projection_on_xy({rectangle(3.0, 3.0, 6.0, 6.0)}, "F1:side9", 1.0)); },
      "no face F1:side9"));
}


// Tapers (mitcad#1547). The 10 x 10 square about the sketch's origin on a
// plane tangent to a cylinder of radius 25 about Z at +Y, raised 45 to the
// radius 70: at 0 its top is 10 * 70 / 25 = 28 wide around the axis and 10
// along it; a taper t moves the top's edges d = 45 tan(t) outwards within
// the top cylinder, so the top is 28 + 2 d by 10 + 2 d (positive widens,
// as an extrusion's taper).
EmbossWrapSpec tapered_square(double taper) {
  EmbossWrapSpec spec;
  spec.feature = "F5";
  spec.frame.origin = gp_Pnt(0.0, 25.0, 0.0);
  spec.frame.x_axis = gp_Dir(-1.0, 0.0, 0.0);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.regions = {rectangle(-5.0, -5.0, 10.0, 10.0)};
  spec.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
  spec.radius = 25.0;
  spec.to_radius = 70.0;
  spec.taper = taper;
  return spec;
}

// The volume between a patch of the cylinder of radius r (half angle a0
// about the axis, half height h0) and one of the cylinder R > r (a1, h1),
// joined by the planes through the axis-parallel edges and ruled walls
// between the arcs by the arcs' angles, by the divergence theorem (V = 1/3
// of the flux of the position through the faces). A ruled face S = (1 - v)
// B(s) + v T(s) contributes 1/6 of the integral of det[B, B', T] +
// det[B, T', T] over s.
double patch_volume(double r, double a0, double h0, double R, double a1, double h1) {
  const double top = R * (2.0 * a1 * R) * (2.0 * h1) / 3.0;
  const double foot = -r * (2.0 * a0 * r) * (2.0 * h0) / 3.0;
  const double planes = -2.0 / 3.0 * r * R * std::sin(a1 - a0) * (h0 + h1);
  // Gauss-Legendre on 64 pieces of s in [-1, 1].
  static constexpr double kNodes[4] = {-0.8611363115940526, -0.3399810435848563, 0.3399810435848563,
                                       0.8611363115940526};
  static constexpr double kWeights[4] = {0.3478548451374538, 0.6521451548625461, 0.6521451548625461,
                                         0.3478548451374538};
  const auto det = [](const gp_Vec& a, const gp_Vec& b, const gp_Vec& c) { return a.Dot(b.Crossed(c)); };
  double ruled = 0.0;
  constexpr int kPieces = 64;
  for (int i = 0; i < kPieces; ++i) {
    for (int k = 0; k < 4; ++k) {
      const double s = -1.0 + 2.0 * (i + (kNodes[k] + 1.0) / 2.0) / kPieces;
      const gp_Vec b(r * std::cos(a0 * s), r * std::sin(a0 * s), h0);
      const gp_Vec db(-r * a0 * std::sin(a0 * s), r * a0 * std::cos(a0 * s), 0.0);
      const gp_Vec t(R * std::cos(a1 * s), R * std::sin(a1 * s), h1);
      const gp_Vec dt(-R * a1 * std::sin(a1 * s), R * a1 * std::cos(a1 * s), 0.0);
      ruled += kWeights[k] * (det(b, db, t) + det(b, dt, t)) * (2.0 / kPieces) / 2.0;
    }
  }
  // Two such walls, at the top and the bottom of the square alike.
  return top + foot + planes + 2.0 * std::abs(ruled) / 6.0;
}

// The volume of the wrapped 10 x 10 square tapered as above (the foot's
// half width w and half height h on the cylinder r, the top on R, its edges
// moved by `shift`).
double tapered_volume(double r, double R, double w, double h, double shift) {
  const double a = w / r;
  const double moved = (w * R / r + shift) / R;
  return r < R ? patch_volume(r, a, h, R, moved, h + shift) : patch_volume(R, moved, h + shift, r, a, h);
}

void test_a_tapered_square_on_a_cylinder() {
  // -10 degrees collapses the square along the axis (10 - 2 * 45 tan(10
  // deg) < 0): test_taper_refusals; -5 narrows it.
  for (const double degrees : {0.0, 10.0, -5.0}) {
    const double taper = degrees * test::kPi / 180.0;
    const double shift = 45.0 * std::tan(taper);
    const ShapePtr tool = wrapped_emboss(tapered_square(taper));
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 1);
    CHECK(tool->find_faces("F5:start(r1)").size() == 1);
    for (int i = 1; i <= 4; ++i) {
      CHECK(tool->find_faces("F5:side(r1c" + std::to_string(i) + ")").size() == 1);
    }
    // The top's edges: two arcs around the axis, two lines along it.
    double around = 0.0;
    double along = 0.0;
    double arc_length = 0.0;
    double line_length = 0.0;
    int arcs = 0;
    int lines = 0;
    for (TopExp_Explorer e(face_named(*tool, "F5:end(r1)"), TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
      const gp_Pnt a = curve.Value(curve.FirstParameter());
      const gp_Pnt b = curve.Value(curve.LastParameter());
      const double length = GCPnts_AbscissaPoint::Length(curve);
      if (std::abs(a.Z() - b.Z()) < 1e-9) {
        around = std::max(around, std::abs(length - (28.0 + 2.0 * shift)));
        arc_length = length;
        ++arcs;
      } else {
        along = std::max(along, std::abs(length - (10.0 + 2.0 * shift)));
        line_length = length;
        ++lines;
      }
    }
    std::printf("taper %+.0f deg: top %.9f around (%.9f), %.9f along (%.9f); volume %.9f (%.9f)\n", degrees,
                arc_length, 28.0 + 2.0 * shift, line_length, 10.0 + 2.0 * shift, volume_of(tool->occt()),
                tapered_volume(25.0, 70.0, 5.0, 5.0, shift));
    CHECK(arcs == 2 && lines == 2);
    CHECK(around < 1e-6);
    CHECK(along < 1e-6);
    // At 0 the volume is the untapered one, A (R^2 - r^2) / (2 r).
    if (degrees == 0.0) {
      CHECK_NEAR(volume_of(tool->occt()), wrapped_volume(100.0, 25.0, 70.0));
    }
    CHECK_NEAR(volume_of(tool->occt()), tapered_volume(25.0, 70.0, 5.0, 5.0, shift));
    // Sunk 20 into the rod (to the radius 5): the floor 2 + 2 d' by
    // 10 + 2 d' (d' = 20 tan(t)), a positive taper widening it too; -10
    // degrees closes it (2 - 2 * 3.5).
    if (degrees < 0.0) {
      EmbossWrapSpec closed = tapered_square(taper);
      closed.to_radius = 5.0;
      CHECK(test::throws_with([&] { wrapped_emboss(closed); }, "the taper collapses the profile r1"));
      continue;
    }
    EmbossWrapSpec sunk = tapered_square(taper);
    sunk.to_radius = 5.0;
    const ShapePtr pocket = wrapped_emboss(sunk);
    CHECK(BRepCheck_Analyzer(pocket->occt()).IsValid());
    const double floor_shift = 20.0 * std::tan(taper);
    CHECK_NEAR(volume_of(pocket->occt()), tapered_volume(25.0, 5.0, 5.0, 5.0, floor_shift));
  }
}

// A taper on a cone (half angle 30 degrees): the top's edges lie on the
// top cone, a widening taper adds volume and a narrowing one takes it.
void test_a_tapered_rectangle_on_a_cone() {
  const double angle = test::kPi / 6.0;
  const double r0 = 20.0 * std::tan(angle);
  EmbossWrapSpec spec;
  spec.feature = "F6";
  spec.frame.origin = gp_Pnt(0.0, r0, 20.0);
  spec.frame.x_axis = gp_Dir(-1.0, 0.0, 0.0);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.regions = {rectangle(-3.0, -2.0, 6.0, 4.0)};
  spec.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
  spec.radius = r0;
  spec.to_radius = r0 + 2.0;
  spec.half_angle = angle;
  const double plain = volume_of(wrapped_emboss(spec)->occt());
  for (const double degrees : {15.0, -15.0}) {
    spec.taper = degrees * test::kPi / 180.0;
    const ShapePtr tool = wrapped_emboss(spec);
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 1);
    const double volume = volume_of(tool->occt());
    CHECK(degrees > 0.0 ? volume > plain * 1.05 : volume < plain * 0.95);
    double worst = 0.0;
    for (TopExp_Explorer e(face_named(*tool, "F6:end(r1)"), TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
      for (int i = 0; i <= 64; ++i) {
        const gp_Pnt p = curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 64.0);
        const double distance = std::hypot(p.X(), p.Y()) * std::cos(angle) - p.Z() * std::sin(angle);
        worst = std::max(worst, std::abs(distance - 2.0));
      }
    }
    // The top's area: the rectangle stretched around the axis by the top
    // cone's larger distance from its apex (k = (rho0 + 2 cot(30 deg)) /
    // rho0, rho0 = 2 r0) and moved by m = 2 tan(t) on the development:
    // near (6 k + 2 m)(4 + 2 m).
    const double m = 2.0 * std::tan(spec.taper);
    const double k = (2.0 * r0 + 2.0 / std::tan(angle)) / (2.0 * r0);
    const double area = area_of(face_named(*tool, "F6:end(r1)"));
    std::printf("on the cone %+.0f deg: volume %.6f (untapered %.6f), top %.6f (near %.6f), %.3g off the top cone\n",
                degrees, volume, plain, area, (6.0 * k + 2.0 * m) * (4.0 + 2.0 * m), worst);
    CHECK(worst < 1e-6);
    CHECK(std::abs(area - (6.0 * k + 2.0 * m) * (4.0 + 2.0 * m)) < 0.03 * area);
  }
}

// Curves and letters tapered on a cylinder of radius 15 raised to 16: a
// circle (its top a circle of the moved radius on the development), a
// rectangle with a hole, and the letters "ABC": valid solids, more volume
// widened and less narrowed, named as without the taper.
void test_curves_and_letters_tapered() {
  std::vector<std::pair<double, double>> extents;
  std::vector<Region> regions = text_regions("ABC", 10.0, -10.0, 5.0, extents);
  Region holed = rectangle(-36.0, 5.0, 6.0, 6.0, "r2");
  Region hole = rectangle(-34.0, 7.0, 2.0, 2.0, "h");
  holed.loops.push_back(hole.loops.front());
  Region circle = circle_region(34.0, 8.0, 2.0);
  circle.name = "r3";
  circle.loops.front().segments.front().name = "c9";
  regions.push_back(holed);
  regions.push_back(circle);
  EmbossWrapSpec spec;
  spec.feature = "F5";
  spec.frame.origin = gp_Pnt(0.0, 15.0, 0.0);
  spec.frame.x_axis = gp_Dir(-1.0, 0.0, 0.0);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.regions = regions;
  spec.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
  spec.radius = 15.0;
  spec.to_radius = 16.0;
  const double plain = volume_of(wrapped_emboss(spec)->occt());
  // Short edges of the letters' outlines drop out where the moved edges'
  // corners pass each other (the B's at 5 degrees outwards).
  for (const double degrees : {5.0, -5.0, 20.0, -10.0}) {
    spec.taper = degrees * test::kPi / 180.0;
    const ShapePtr tool = wrapped_emboss(spec);
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 5);
    const double volume = volume_of(tool->occt());
    std::printf("curves and letters %+.0f deg: volume %.6f (untapered %.6f)\n", degrees, volume, plain);
    CHECK(degrees > 0.0 ? volume > plain : volume < plain);
    for (const char* name : {"F5:end(r_A)", "F5:end(r_B)", "F5:end(r_C)", "F5:end(r2)", "F5:end(r3)", "F5:side(c9)",
                             "F5:side(hc1)"}) {
      CHECK(tool->find_faces(name).size() == 1);
    }
    // The circle's top: on the top's development the circle stretched
    // around the axis to an ellipse of half axes 2 * 16 / 15 and 2, moved
    // out by d = tan(t): its area pi a b + d P + pi d^2 (P its perimeter,
    // by Ramanujan's formula, closer than 1e-9 at this shape), within the
    // 1e-5 the fitted curves and the area's integration keep.
    const double a = 2.0 * 16.0 / 15.0;
    const double b = 2.0;
    const double h = (a - b) * (a - b) / ((a + b) * (a + b));
    const double perimeter = test::kPi * (a + b) * (1.0 + 3.0 * h / (10.0 + std::sqrt(4.0 - 3.0 * h)));
    const double d = std::tan(spec.taper);
    GProp_GProps top;
    BRepGProp::SurfaceProperties(face_named(*tool, "F5:end(r3)"), top, 1.0e-10);
    CHECK_NEAR_TOL(top.Mass(), test::kPi * a * b + d * perimeter + test::kPi * d * d, 1e-5);
  }
}

// The top face of a 40 x 40 x 10 box about the Z axis, named F1:top.
Shape box_with_top() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-20.0, -20.0, 0.0), 40.0, 40.0, 10.0).Shape();
  TopoDS_Face top;
  for (TopExp_Explorer f(box, TopAbs_FACE); f.More(); f.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f.Current(), props);
    if (std::abs(props.CentreOfMass().Z() - 10.0) < 1e-9) {
      top = TopoDS::Face(f.Current());
    }
  }
  CHECK(!top.IsNull());
  return Shape(box, {{top, {"F1:top"}}});
}

// A 10 x 10 square projected onto a box's top and raised or sunk 5 with a
// taper of 5 degrees either way: a frustum of the square and the square
// grown by 5 tan(t) on each side (positive widens), as an extrusion of it
// with that taper.
void test_a_tapered_square_projected_onto_a_plane() {
  const Shape body = box_with_top();
  for (const double depth : {5.0, -5.0}) {
    for (const double degrees : {5.0, -5.0}) {
      EmbossProjectSpec spec = projection_on_xy({rectangle(-5.0, -5.0, 10.0, 10.0)}, "F1:top", depth);
      spec.taper = degrees * test::kPi / 180.0;
      const ShapePtr tool = projected_emboss(body, spec);
      CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
      CHECK(solids(tool->occt()) == 1);
      const double side = 10.0 + 2.0 * 5.0 * std::tan(spec.taper);
      const double frustum = 5.0 / 3.0 * (100.0 + side * side + 10.0 * side);
      std::printf("projected %+.0f deg, depth %+.0f: volume %.9f (frustum %.9f)\n", degrees, depth,
                  volume_of(tool->occt()), frustum);
      CHECK_NEAR(volume_of(tool->occt()), frustum);
      CHECK_NEAR(area_of(face_named(*tool, "F3:end(r1)")), side * side);
      CHECK_NEAR(area_of(face_named(*tool, "F3:start(r1)")), 100.0);
      for (int i = 1; i <= 4; ++i) {
        CHECK(!tool->find_faces("F3:side(r1c" + std::to_string(i) + ")").empty());
      }
    }
  }
}

// Letters projected onto a box's top and raised 1 with a taper either way:
// valid solids, named as without it, more volume widened and less
// narrowed (short edges of the outlines drop out where they must).
void test_letters_projected_and_tapered() {
  const Shape body = box_with_top();
  std::vector<std::pair<double, double>> extents;
  EmbossProjectSpec spec = projection_on_xy(text_regions("ABC", 10.0, -15.0, -5.0, extents), "F1:top", 1.0);
  const double plain = volume_of(projected_emboss(body, spec)->occt());
  for (const double degrees : {5.0, -5.0}) {
    spec.taper = degrees * test::kPi / 180.0;
    const ShapePtr tool = projected_emboss(body, spec);
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 3);
    const double volume = volume_of(tool->occt());
    std::printf("letters projected %+.0f deg: volume %.6f (untapered %.6f)\n", degrees, volume, plain);
    CHECK(degrees > 0.0 ? volume > plain : volume < plain);
    for (const char* name : {"F3:end(r_A)", "F3:end(r_B)", "F3:end(r_C)", "F3:start(r_B)"}) {
      CHECK(!tool->find_faces(name).empty());
    }
  }
}

// A square projected onto a ball and raised 1 with a taper: a valid solid
// on the sphere's normals, more volume with a widening taper.
void test_a_tapered_square_projected_onto_a_sphere() {
  const Shape body = ball();
  EmbossProjectSpec spec = projection_on_xy({rectangle(-3.0, -3.0, 6.0, 6.0)}, "F1:side0", 1.0);
  const double plain = volume_of(projected_emboss(body, spec)->occt());
  for (const double degrees : {20.0, -20.0}) {
    spec.taper = degrees * test::kPi / 180.0;
    const ShapePtr tool = projected_emboss(body, spec);
    CHECK(BRepCheck_Analyzer(tool->occt()).IsValid());
    CHECK(solids(tool->occt()) == 1);
    const double volume = volume_of(tool->occt());
    std::printf("on the ball %+.0f deg: volume %.9f (untapered %.9f)\n", degrees, volume, plain);
    CHECK(degrees > 0.0 ? volume > plain : volume < plain);
    CHECK(tool->find_faces("F3:end(r1)").size() == 1);
  }
}

// A taper that leans the walls in until the top would cross itself fails
// naming the region; one beyond a right angle is refused.
void test_taper_refusals() {
  // 45 tan(10 deg) = 7.9 mm in from each side of the square's 10 mm along
  // the axis, and 255 mm at 80 degrees.
  for (const double degrees : {-10.0, -80.0}) {
    CHECK(test::throws_with([&] { wrapped_emboss(tapered_square(degrees * test::kPi / 180.0)); },
                            "the taper collapses the profile r1"));
  }
  CHECK(test::throws_with([] { wrapped_emboss(tapered_square(test::kPi / 2.0)); }, "taper angle"));
  // 5 tan(60 deg) = 8.7 mm in from each side of the projected 10 mm square.
  const Shape body = box_with_top();
  EmbossProjectSpec spec = projection_on_xy({rectangle(-5.0, -5.0, 10.0, 10.0)}, "F1:top", 5.0);
  spec.taper = -60.0 * test::kPi / 180.0;
  CHECK(test::throws_with([&] { projected_emboss(body, spec); }, "the taper collapses the profile r1"));
}
} // namespace

int main() {
  test::guarded("a rectangle raised and sunk", test_a_rectangle_raised_and_sunk);
  test::guarded("curves, holes and a rotated axis", test_curves_holes_and_a_rotated_axis);
  test::guarded("a bore and a sketch off the cylinder", test_a_bore_and_a_sketch_off_the_cylinder);
  test::guarded("refusals", test_refusals);
  test::guarded("text wrapped on a cylinder", test_text_wrapped_on_a_cylinder);
  test::guarded("a rectangle developed on a cone", test_a_rectangle_developed_on_a_cone);
  test::guarded("a square projected onto a sphere", test_a_square_projected_onto_a_sphere);
  test::guarded("a square projected onto a B-spline face", test_a_square_projected_onto_a_bspline_face);
  test::guarded("a direction shifts the projection", test_a_direction_shifts_the_projection);
  test::guarded("projection refusals", test_projection_refusals);
  test::guarded("a slanted direction takes the near side of a ball",
                test_a_slanted_direction_takes_the_near_side_of_a_ball);
  test::guarded("a point inside a sliver", test_a_point_inside_a_sliver);
  test::guarded("a tapered square on a cylinder", test_a_tapered_square_on_a_cylinder);
  test::guarded("a tapered rectangle on a cone", test_a_tapered_rectangle_on_a_cone);
  test::guarded("curves and letters tapered", test_curves_and_letters_tapered);
  test::guarded("a tapered square projected onto a plane", test_a_tapered_square_projected_onto_a_plane);
  test::guarded("a tapered square projected onto a sphere", test_a_tapered_square_projected_onto_a_sphere);
  test::guarded("letters projected and tapered", test_letters_projected_and_tapered);
  test::guarded("taper refusals", test_taper_refusals);
  if (test::failures == 0) {
    std::puts("test_emboss: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
