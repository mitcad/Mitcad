// SPDX-License-Identifier: MIT
// Outlines of faces seen along a direction (mitcad#1265): Project of a
// cylinder's, a tube's or a cone's side face gives the rectangle or the
// trapezoid it is seen as, its seam only where it is on the outline; seen
// along the axis only the circles; a part of a cylinder the generators its
// angle holds; another curved face OCCT's outline.

#include <cmath>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/face_outline.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace test {
namespace {

using mitcad::geometry::ModelCurve;
using mitcad::geometry::ModelCurveKind;
using mitcad::geometry::Shape;

// The comparisons' tolerance (mm).
constexpr double kTolerance = 1e-6;

bool at(const gp_Pnt& p, double x, double y, double z) {
  return p.Distance(gp_Pnt(x, y, z)) <= kTolerance;
}

// A solid's faces named: the first face for which `pick` holds "F1:side",
// the others "F1:other<i>".
template <class Pick>
Shape named(const TopoDS_Shape& solid, Pick pick) {
  std::vector<Shape::NamedFace> faces;
  bool found = false;
  int i = 0;
  for (TopExp_Explorer it(solid, TopAbs_FACE); it.More(); it.Next(), ++i) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (!found && pick(face)) {
      found = true;
      faces.push_back({face, {"F1:side"}});
    } else {
      faces.push_back({face, {"F1:other" + std::to_string(i)}});
    }
  }
  CHECK(found);
  return Shape(solid, faces);
}

bool is_kind(const TopoDS_Face& face, GeomAbs_SurfaceType kind) {
  return BRepAdaptor_Surface(face).GetType() == kind;
}

std::vector<ModelCurve> of_kind(const std::vector<ModelCurve>& curves, ModelCurveKind kind) {
  std::vector<ModelCurve> out;
  for (const ModelCurve& c : curves) {
    if (c.kind == kind) {
      out.push_back(c);
    }
  }
  return out;
}

// Whether a line from (x0, z0) to (x1, z1) seen along Y (y anywhere on it,
// as long as it is the same at both ends) is among the lines, either way
// round.
bool has_line_seen_along_y(const std::vector<ModelCurve>& lines, double x0, double z0, double x1, double z1) {
  for (const ModelCurve& c : lines) {
    const auto is = [](const gp_Pnt& p, double x, double z) {
      return std::abs(p.X() - x) <= kTolerance && std::abs(p.Z() - z) <= kTolerance;
    };
    if ((is(c.start, x0, z0) && is(c.end, x1, z1)) || (is(c.start, x1, z1) && is(c.end, x0, z0))) {
      return true;
    }
  }
  return false;
}

// A cylinder of radius r and height h standing on the origin along +Z,
// its seam toward `seam`, seen along +Y (a sketch on the XZ plane): its
// circles seen edge on and its two silhouettes, the rectangle; the seam a
// line of its own only where it is a silhouette.
void check_rectangle(const Shape& shape, double r, double h) {
  const std::vector<ModelCurve> curves = mitcad::geometry::face_outline(shape, "F1:side", gp_Dir(0.0, 1.0, 0.0));
  const std::vector<ModelCurve> lines = of_kind(curves, ModelCurveKind::Line);
  const std::vector<ModelCurve> circles = of_kind(curves, ModelCurveKind::Conic);
  CHECK(curves.size() == 4);
  CHECK(lines.size() == 2);
  CHECK(circles.size() == 2);
  CHECK(has_line_seen_along_y(lines, r, 0.0, r, h));
  CHECK(has_line_seen_along_y(lines, -r, 0.0, -r, h));
  for (const ModelCurve& line : lines) {
    // On the outline: where the cylinder's normal is along X.
    CHECK(std::abs(line.start.Y()) <= kTolerance && std::abs(line.end.Y()) <= kTolerance);
  }
  for (const ModelCurve& c : circles) {
    CHECK_NEAR_TOL(c.major, r, 1e-9);
  }
}

void test_a_cylinder_seen_from_the_side_is_its_rectangle() {
  // The seam toward the view's side (on the outline), toward the viewer (at
  // right angles to it: a line down the middle that is left out) and in
  // between.
  for (const gp_Dir& seam : {gp_Dir(1.0, 0.0, 0.0), gp_Dir(0.0, 1.0, 0.0), gp_Dir(-0.6, 0.8, 0.0)}) {
    const gp_Ax2 axes(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0), seam);
    const Shape shape = named(BRepPrimAPI_MakeCylinder(axes, 31.6, 200.0).Shape(),
                              [](const TopoDS_Face& face) { return is_kind(face, GeomAbs_Cylinder); });
    check_rectangle(shape, 31.6, 200.0);
  }
}

void test_a_tubes_inner_face_seen_from_the_side_is_its_rectangle() {
  const gp_Ax2 outer(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(0.0, 1.0, 0.0));
  const gp_Ax2 inner(gp_Pnt(0.0, 0.0, -1.0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(0.0, -1.0, 0.0));
  const TopoDS_Shape tube = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(outer, 10.0, 50.0).Shape(),
                                            BRepPrimAPI_MakeCylinder(inner, 8.0, 52.0).Shape())
                                .Shape();
  const Shape shape = named(tube, [](const TopoDS_Face& face) {
    const BRepAdaptor_Surface surface(face);
    return surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - 8.0) < 1e-9;
  });
  check_rectangle(shape, 8.0, 50.0);
}

void test_a_cone_seen_from_the_side_is_its_trapezoid() {
  // Radius 10 at the base, 5 at the top, 50 high; the seam toward the
  // viewer.
  const gp_Ax2 axes(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(0.0, 1.0, 0.0));
  const Shape shape = named(BRepPrimAPI_MakeCone(axes, 10.0, 5.0, 50.0).Shape(),
                            [](const TopoDS_Face& face) { return is_kind(face, GeomAbs_Cone); });
  const std::vector<ModelCurve> curves = mitcad::geometry::face_outline(shape, "F1:side", gp_Dir(0.0, 1.0, 0.0));
  const std::vector<ModelCurve> lines = of_kind(curves, ModelCurveKind::Line);
  CHECK(curves.size() == 4);
  CHECK(of_kind(curves, ModelCurveKind::Conic).size() == 2);
  CHECK(lines.size() == 2);
  CHECK(has_line_seen_along_y(lines, 10.0, 0.0, 5.0, 50.0));
  CHECK(has_line_seen_along_y(lines, -10.0, 0.0, -5.0, 50.0));
}

void test_a_round_face_seen_along_its_axis_is_its_circles() {
  // Seen along Z (a sketch on XY): the two circles, no silhouettes, and the
  // seam, seen end on, left out.
  const gp_Ax2 axes(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(1.0, 0.0, 0.0));
  const Shape cylinder = named(BRepPrimAPI_MakeCylinder(axes, 10.0, 50.0).Shape(),
                               [](const TopoDS_Face& face) { return is_kind(face, GeomAbs_Cylinder); });
  for (const gp_Dir& along : {gp_Dir(0.0, 0.0, 1.0), gp_Dir(0.0, 0.0, -1.0)}) {
    const std::vector<ModelCurve> curves = mitcad::geometry::face_outline(cylinder, "F1:side", along);
    CHECK(curves.size() == 2);
    CHECK(of_kind(curves, ModelCurveKind::Conic).size() == 2);
  }
  const Shape cone = named(BRepPrimAPI_MakeCone(axes, 10.0, 5.0, 50.0).Shape(),
                           [](const TopoDS_Face& face) { return is_kind(face, GeomAbs_Cone); });
  const std::vector<ModelCurve> curves = mitcad::geometry::face_outline(cone, "F1:side", gp_Dir(0.0, 0.0, 1.0));
  CHECK(curves.size() == 2);
  CHECK(of_kind(curves, ModelCurveKind::Conic).size() == 2);
  // Seen almost along the axis, from inside the cone's angle (its half
  // angle is atan(5 / 50)): no silhouettes either.
  const gp_Dir steep(0.05, 0.0, 1.0);
  const TopoDS_Face& side = cone.face(cone.find_faces("F1:side").front());
  CHECK(mitcad::geometry::face_silhouettes(side, steep).empty());
}

void test_a_part_of_a_cylinder_has_the_generators_its_angle_holds() {
  // A half cylinder from +X through +Y to -X (its angle 0 to pi).
  const gp_Ax2 axes(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(1.0, 0.0, 0.0));
  const Shape half = named(BRepPrimAPI_MakeCylinder(axes, 10.0, 50.0, kPi).Shape(),
                           [](const TopoDS_Face& face) { return is_kind(face, GeomAbs_Cylinder); });
  const TopoDS_Face& face = half.face(half.find_faces("F1:side").front());
  // Seen along X: one generator inside its angle, at +Y.
  const std::vector<ModelCurve> one = mitcad::geometry::face_silhouettes(face, gp_Dir(1.0, 0.0, 0.0));
  CHECK(one.size() == 1);
  CHECK(!one.empty() && one[0].kind == ModelCurveKind::Line &&
        ((at(one[0].start, 0.0, 10.0, 0.0) && at(one[0].end, 0.0, 10.0, 50.0)) ||
         (at(one[0].start, 0.0, 10.0, 50.0) && at(one[0].end, 0.0, 10.0, 0.0))));
  // Its outline seen along X: the arcs, its two straight sides and the
  // generator.
  const std::vector<ModelCurve> outline = mitcad::geometry::face_outline(half, "F1:side", gp_Dir(1.0, 0.0, 0.0));
  CHECK(of_kind(outline, ModelCurveKind::Line).size() == 3);
  CHECK(of_kind(outline, ModelCurveKind::Conic).size() == 2);
  // Seen along Y: the two generators are its straight sides, which the
  // outline lists once.
  CHECK(mitcad::geometry::face_silhouettes(face, gp_Dir(0.0, 1.0, 0.0)).size() == 2);
  const std::vector<ModelCurve> sides = mitcad::geometry::face_outline(half, "F1:side", gp_Dir(0.0, 1.0, 0.0));
  CHECK(sides.size() == 4);
  CHECK(has_line_seen_along_y(of_kind(sides, ModelCurveKind::Line), 10.0, 0.0, 10.0, 50.0));
  CHECK(has_line_seen_along_y(of_kind(sides, ModelCurveKind::Line), -10.0, 0.0, -10.0, 50.0));
  // A quarter (0 to pi/2) seen along (1, 1, 0): its generators would be at
  // 3pi/4 and -pi/4, off the face.
  const Shape quarter = named(BRepPrimAPI_MakeCylinder(axes, 10.0, 50.0, kPi / 2.0).Shape(),
                              [](const TopoDS_Face& candidate) { return is_kind(candidate, GeomAbs_Cylinder); });
  const TopoDS_Face& piece = quarter.face(quarter.find_faces("F1:side").front());
  CHECK(mitcad::geometry::face_silhouettes(piece, gp_Dir(1.0, 1.0, 0.0)).empty());
}

void test_a_cylinder_cut_slanted_is_clipped_to_its_boundary() {
  // A cylinder cut by a plane through its top at 45 degrees (z = 40 + x):
  // seen along Y, each silhouette runs from the base to the cut, 30 high
  // at x = -10 and 50 at x = 10.
  const gp_Ax2 axes(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0), gp_Dir(0.0, 1.0, 0.0));
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(axes, 10.0, 80.0).Shape();
  // What lies above the plane: a wide cylinder standing on it, along its
  // normal, that holds everything of the first one above it.
  const gp_Ax2 tilted(gp_Pnt(0.0, 0.0, 40.0), gp_Dir(-1.0, 0.0, 1.0), gp_Dir(1.0, 0.0, 1.0));
  const TopoDS_Shape above = BRepPrimAPI_MakeCylinder(tilted, 100.0, 100.0).Shape();
  const TopoDS_Shape cut = BRepAlgoAPI_Cut(cylinder, above).Shape();
  const Shape shape = named(cut, [](const TopoDS_Face& face) { return is_kind(face, GeomAbs_Cylinder); });
  const TopoDS_Face& side = shape.face(shape.find_faces("F1:side").front());
  const std::vector<ModelCurve> lines = mitcad::geometry::face_silhouettes(side, gp_Dir(0.0, 1.0, 0.0));
  CHECK(lines.size() == 2);
  CHECK(has_line_seen_along_y(lines, 10.0, 0.0, 10.0, 50.0));
  CHECK(has_line_seen_along_y(lines, -10.0, 0.0, -10.0, 30.0));
}

void test_a_spheres_outline_is_its_great_circle() {
  // Another curved face: OCCT's outline, the great circle at right angles
  // to the view (in pieces where the sphere's seam cuts it).
  const Shape sphere = named(BRepPrimAPI_MakeSphere(10.0).Shape(),
                             [](const TopoDS_Face& face) { return is_kind(face, GeomAbs_Sphere); });
  const gp_Dir view(0.0, 0.6, 0.8);
  const TopoDS_Face& face = sphere.face(sphere.find_faces("F1:side").front());
  const std::vector<ModelCurve> curves = mitcad::geometry::face_silhouettes(face, view);
  CHECK(!curves.empty());
  // On the circle: 10 from the centre, in the plane through it at right
  // angles to the view.
  const auto on_circle = [&view](const gp_Pnt& p) {
    return std::abs(p.Distance(gp_Pnt(0.0, 0.0, 0.0)) - 10.0) <= 1e-5 &&
           std::abs(gp_Vec(p.XYZ()).Dot(gp_Vec(view))) <= 1e-5;
  };
  double sweep = 0.0;
  for (const ModelCurve& c : curves) {
    if (c.kind == ModelCurveKind::Conic) {
      CHECK_NEAR_TOL(c.major, 10.0, 1e-9);
      CHECK_NEAR_TOL(c.minor, 10.0, 1e-9);
      CHECK(c.center.Distance(gp_Pnt(0.0, 0.0, 0.0)) <= kTolerance);
      CHECK(std::abs(std::abs(gp_Vec(c.normal).Dot(gp_Vec(view))) - 1.0) <= 1e-9);
      sweep += c.closed ? 2.0 * kPi : c.last - c.first;
    } else {
      // A B-spline's ends are its end poles.
      CHECK(c.kind == ModelCurveKind::BSpline && !c.poles.empty());
      if (!c.poles.empty()) {
        CHECK(on_circle(c.poles.front()) && on_circle(c.poles.back()));
      }
    }
  }
  CHECK(sweep <= 2.0 * kPi + 1e-9);
  // A plane has none (a disc's end, seen edge on or not).
  const Shape disc = named(BRepPrimAPI_MakeCylinder(10.0, 5.0).Shape(),
                           [](const TopoDS_Face& candidate) { return is_kind(candidate, GeomAbs_Plane); });
  const TopoDS_Face& flat = disc.face(disc.find_faces("F1:side").front());
  CHECK(mitcad::geometry::face_silhouettes(flat, gp_Dir(0.0, 1.0, 0.0)).empty());
}

} // namespace

void face_outline_tests() {
  guarded("test_a_cylinder_seen_from_the_side_is_its_rectangle", test_a_cylinder_seen_from_the_side_is_its_rectangle);
  guarded("test_a_tubes_inner_face_seen_from_the_side_is_its_rectangle",
          test_a_tubes_inner_face_seen_from_the_side_is_its_rectangle);
  guarded("test_a_cone_seen_from_the_side_is_its_trapezoid", test_a_cone_seen_from_the_side_is_its_trapezoid);
  guarded("test_a_round_face_seen_along_its_axis_is_its_circles", test_a_round_face_seen_along_its_axis_is_its_circles);
  guarded("test_a_part_of_a_cylinder_has_the_generators_its_angle_holds",
          test_a_part_of_a_cylinder_has_the_generators_its_angle_holds);
  guarded("test_a_cylinder_cut_slanted_is_clipped_to_its_boundary",
          test_a_cylinder_cut_slanted_is_clipped_to_its_boundary);
  guarded("test_a_spheres_outline_is_its_great_circle", test_a_spheres_outline_is_its_great_circle);
}

} // namespace test
