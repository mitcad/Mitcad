// SPDX-License-Identifier: MIT
// Construction geometry queries: surfaces, curves and points of named
// sub-shapes, paths and datum display shapes.

#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Line.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Lin.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

bool at(const gp_Pnt& p, double x, double y, double z) {
  return p.Distance(gp_Pnt(x, y, z)) < 1e-7;
}

bool along(const gp_Dir& d, double x, double y, double z) {
  return d.IsEqual(gp_Dir(x, y, z), 1e-9);
}

// The block x 0..60, y 0..40, z 0..20 of F2 (lines c1..c4) and the
// cylinder of radius 5 and height 10 at the origin of F3 (circle c5).
const ShapePtr& block() {
  static const ShapePtr shape = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  return shape;
}

const ShapePtr& cylinder() {
  static const ShapePtr shape = extrude("F3", {circle(5, 0, 0, 5)}, 0, 10);
  return shape;
}

void test_surfaces() {
  const SurfaceDescription top = face_geometry(*block(), end_cap("F2", 1));
  CHECK(top.type == "plane");
  CHECK(along(top.axis, 0, 0, 1));
  CHECK(at(top.origin, 0, 0, 20));
  const SurfaceDescription bottom = face_geometry(*block(), start_cap("F2", 1));
  CHECK(along(bottom.axis, 0, 0, -1) && at(bottom.origin, 0, 0, 0));
  // Side c1 is the face y = 0, its outward normal -Y; c2 is x = 60.
  const SurfaceDescription front = face_geometry(*block(), side("F2", 1, 0));
  CHECK(along(front.axis, 0, -1, 0) && at(front.origin, 0, 0, 0));
  const SurfaceDescription right = face_geometry(*block(), side("F2", 1, 1));
  CHECK(along(right.axis, 1, 0, 0) && at(right.origin, 60, 0, 0));

  const SurfaceDescription wall = face_geometry(*cylinder(), face_name("F3", "side", "c5"));
  CHECK(wall.type == "cylinder");
  CHECK(near(wall.radius, 5.0));
  CHECK(along(wall.axis, 0, 0, 1));
  CHECK(at(wall.origin, 0, 0, 5));
  CHECK(throws_with([] { face_geometry(*block(), "F2:side(c9)"); }, "the body has no such face"));
}

void test_split_faces_on_one_plane() {
  // A groove across the top splits it into two pieces on one plane.
  const ShapePtr groove = extrude("F4", {rectangle(11, 25, -1, 10, 42)}, 10, 21);
  const BooleanResult cut = boolean(BooleanOp::Cut, {block().get()}, *groove);
  CHECK(cut.pieces.size() == 1);
  const Shape& grooved = *cut.pieces.front().shape;
  CHECK(grooved.find_faces(end_cap("F2", 1)).size() == 2);
  const SurfaceDescription top = face_geometry(grooved, end_cap("F2", 1));
  CHECK(top.type == "plane" && at(top.origin, 0, 0, 20));
  const SurfacePoint touch = face_point_normal(grooved, end_cap("F2", 1), gp_Pnt(30, 20, 30));
  CHECK(along(touch.normal, 0, 0, 1));
  CHECK(near(touch.point.Z(), 20.0) && near(std::abs(touch.point.X() - 30.0), 5.0));
}

void test_curves_and_vertices() {
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  const CurveDescription line = edge_geometry(*block(), top_front);
  CHECK(line.type == "line");
  CHECK((at(line.start, 0, 0, 20) && at(line.end, 60, 0, 20)) ||
        (at(line.start, 60, 0, 20) && at(line.end, 0, 0, 20)));
  const std::string rim = edge(face_name("F3", "end", "r{c5}"), face_name("F3", "side", "c5"));
  const CurveDescription circle = edge_geometry(*cylinder(), rim);
  CHECK(circle.type == "circle");
  CHECK(at(circle.center, 0, 0, 10) && near(circle.radius, 5.0));
  CHECK(circle.start.Distance(circle.end) < 1e-7);
  CHECK(throws_with([] { edge_geometry(*block(), "E{F2:a|F2:b}"); }, "no such edge"));

  const std::string corner =
      mitcad::geometry::make_vertex_name({side("F2", 1, 0), side("F2", 1, 1), start_cap("F2", 1)});
  CHECK(at(vertex_point(*block(), corner), 60, 0, 0));
}

void test_paths() {
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  const std::string top_right = edge(end_cap("F2", 1), side("F2", 1, 1));
  const gp_Pnt none;
  // Front then right: from (0, 0, 20) to (60, 0, 20) to (60, 40, 20).
  const std::vector<std::string> path{top_front, top_right};
  const PathPoint half = path_point(*block(), path, PathAt::Fraction, 0.5, none);
  CHECK(at(half.point, 50, 0, 20) && along(half.tangent, 1, 0, 0));
  const PathPoint later = path_point(*block(), path, PathAt::Length, 80.0, none);
  CHECK(at(later.point, 60, 20, 20) && along(later.tangent, 0, 1, 0));
  const PathPoint beyond = path_point(*block(), path, PathAt::Fraction, 1.1, none);
  CHECK(at(beyond.point, 60, 50, 20));
  const PathPoint before = path_point(*block(), path, PathAt::Length, -5.0, none);
  CHECK(at(before.point, -5, 0, 20));
  // The other way round.
  const std::vector<std::string> back{top_right, top_front};
  const PathPoint other = path_point(*block(), back, PathAt::Fraction, 0.5, none);
  CHECK(at(other.point, 50, 0, 20) && along(other.tangent, -1, 0, 0));
  const PathPoint nearest = path_point(*block(), path, PathAt::Near, 0.0, gp_Pnt(30, -5, 25));
  CHECK(at(nearest.point, 30, 0, 20) && along(nearest.tangent, 1, 0, 0));
  const std::string bottom_back = edge(start_cap("F2", 1), side("F2", 1, 2));
  CHECK(throws_with(
      [&] { path_point(*block(), {top_front, bottom_back}, PathAt::Fraction, 0.5, none); },
      "not connected"));
}

void test_face_points_and_display() {
  const SurfacePoint on = face_point_normal(*cylinder(), face_name("F3", "side", "c5"),
                                            gp_Pnt(20, 0, 5));
  CHECK(at(on.point, 5, 0, 5) && along(on.normal, 1, 0, 0));
  const ShapePtr plane = plane_shape(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 10);
  CHECK(plane->occt().ShapeType() == TopAbs_FACE);
  CHECK(near(mass_properties(*plane).area, 100.0));
  CHECK(bounding_box(*plane).max.Z() < 5.0 + 1e-7);
  const ShapePtr axis = axis_shape(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0), 8);
  CHECK(axis->occt().ShapeType() == TopAbs_EDGE);
  CHECK(near(bounding_box(*axis).min.Y(), -4.0));
  CHECK(point_shape(gp_Pnt(1, 2, 3))->occt().ShapeType() == TopAbs_VERTEX);
  CHECK(throws_with([] { plane_shape(gp_Pnt(), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0), 0); }, "size"));
}

// A face on the whole of a surface's parameter ranges given.
TopoDS_Face face_on(const occ::handle<Geom_Surface>& surface, double u0, double u1, double v0, double v1) {
  BRepBuilderAPI_MakeFace maker(surface, u0, u1, v0, v1, 1e-7);
  CHECK(maker.IsDone());
  return maker.Face();
}

// A face on a surface converted to a B-spline over the ranges given (as the
// walls of a sheet body's holes are stored).
TopoDS_Face spline_face(const occ::handle<Geom_Surface>& surface, double u0, double u1, double v0, double v1) {
  const occ::handle<Geom_BSplineSurface> spline =
      GeomConvert::SurfaceToBSplineSurface(new Geom_RectangularTrimmedSurface(surface, u0, u1, v0, v1));
  double a0 = 0.0;
  double a1 = 0.0;
  double b0 = 0.0;
  double b1 = 0.0;
  spline->Bounds(a0, a1, b0, b1);
  return face_on(spline, a0, a1, b0, b1);
}

// A shape of one face with one name.
Shape named_face(const TopoDS_Face& face, const std::string& name) {
  std::vector<Shape::NamedFace> faces;
  faces.push_back({face, {name}});
  return Shape(face, faces);
}

std::string kind_of(const TopoDS_Face& face) {
  const char* kind = analytic_kind(face);
  return kind != nullptr ? kind : "none";
}

// Faces stored as other surfaces than the analytic ones are the cylinder,
// sphere or torus they lie on (mitcad#1855: the wall of a hole cut in a sheet
// body's flange is a B-spline cylinder, which no pick called round, so Axis
// Through Cylinder and the joints refused it).
void test_round_faces_stored_as_other_surfaces() {
  // A B-spline cylinder of radius 4 about the line x = 1, y = 2, z 3..5.
  const gp_Ax3 frame(gp_Pnt(1, 2, 3), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0));
  const TopoDS_Face wall = spline_face(new Geom_CylindricalSurface(frame, 4.0), 0.0, 2.0 * kPi, 0.0, 2.0);
  CHECK(BRepAdaptor_Surface(wall).GetType() == GeomAbs_BSplineSurface);
  CHECK(kind_of(wall) == "cylinder");
  const occ::handle<Geom_Surface> found = analytic_surface(wall);
  CHECK(!found.IsNull());
  if (!found.IsNull()) {
    const GeomAdaptor_Surface surface(found);
    CHECK(surface.GetType() == GeomAbs_Cylinder);
    if (surface.GetType() == GeomAbs_Cylinder) {
      CHECK(std::abs(surface.Cylinder().Radius() - 4.0) < 1e-6);
      CHECK(surface.Cylinder().Axis().Direction().IsParallel(gp_Dir(0, 0, 1), 1e-9));
      CHECK(gp_Lin(surface.Cylinder().Axis()).Distance(gp_Pnt(1, 2, 0)) < 1e-6);
    }
  }
  // face_geometry reads it so: the axis point nearest the wall's middle,
  // the axis along +Z.
  const Shape named = named_face(wall, "W");
  const SurfaceDescription described = face_geometry(named, "W");
  CHECK(described.type == "cylinder");
  CHECK(std::abs(described.radius - 4.0) < 1e-6);
  CHECK(along(described.axis, 0, 0, 1));
  CHECK(described.origin.Distance(gp_Pnt(1, 2, 4)) < 1e-6);
  // Its circular edges are circles.
  int circles = 0;
  for (int i = 0; i < named.edge_count(); ++i) {
    circles += is_circular(named.edge(i)) ? 1 : 0;
  }
  CHECK(circles == 2);

  // An extrusion of a circle of radius 3 along its axis (z 0..5).
  const occ::handle<Geom_Circle> base_circle = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 3.0);
  const TopoDS_Face extruded =
      face_on(new Geom_SurfaceOfLinearExtrusion(base_circle, gp_Dir(0, 0, 1)), 0.0, 2.0 * kPi, 0.0, 5.0);
  CHECK(BRepAdaptor_Surface(extruded).GetType() == GeomAbs_SurfaceOfExtrusion);
  CHECK(kind_of(extruded) == "cylinder");
  const SurfaceDescription extrusion = face_geometry(named_face(extruded, "E"), "E");
  CHECK(extrusion.type == "cylinder" && std::abs(extrusion.radius - 3.0) < 1e-6 && along(extrusion.axis, 0, 0, 1));

  // A revolution of a line parallel to the axis at x = 2 (a cylinder).
  const TopoDS_Face revolved =
      face_on(new Geom_SurfaceOfRevolution(new Geom_Line(gp_Pnt(2, 0, 0), gp_Dir(0, 0, 1)),
                                           gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))),
              0.0, 2.0 * kPi, 0.0, 5.0);
  CHECK(kind_of(revolved) == "cylinder");

  // A B-spline torus of radii 5 and 1 about Z.
  const TopoDS_Face ring = spline_face(new Geom_ToroidalSurface(gp_Ax3(gp::XOY()), 5.0, 1.0), 0.0, 2.0 * kPi, 0.0,
                                       2.0 * kPi);
  CHECK(kind_of(ring) == "torus");
  const SurfaceDescription torus = face_geometry(named_face(ring, "T"), "T");
  CHECK(torus.type == "torus" && std::abs(torus.radius - 5.0) < 1e-6 && std::abs(torus.minor_radius - 1.0) < 1e-6);

  // Not round: an extrusion of an ellipse, a wavy B-spline.
  const occ::handle<Geom_Ellipse> base_ellipse = new Geom_Ellipse(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 4.0, 2.0);
  const TopoDS_Face oval = face_on(new Geom_SurfaceOfLinearExtrusion(base_ellipse, gp_Dir(0, 0, 1)), 0.0, 2.0 * kPi, 0.0, 5.0);
  CHECK(kind_of(oval) == "none");
  CHECK(analytic_surface(oval).IsNull());
  NCollection_Array2<gp_Pnt> poles(1, 4, 1, 4);
  for (int i = 1; i <= 4; ++i) {
    for (int j = 1; j <= 4; ++j) {
      poles(i, j) = gp_Pnt(i * 10.0, j * 10.0, ((i + 2 * j) % 3) * 4.0);
    }
  }
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  NCollection_Array1<int> multiplicities(1, 2);
  multiplicities(1) = 4;
  multiplicities(2) = 4;
  const TopoDS_Face wavy =
      face_on(new Geom_BSplineSurface(poles, knots, knots, multiplicities, multiplicities, 3, 3), 0.0, 1.0, 0.0, 1.0);
  CHECK(kind_of(wavy) == "none");
  const SurfaceDescription other = face_geometry(named_face(wavy, "S"), "S");
  CHECK(other.type == "bspline");

  // Edges: a half circle stored as a B-spline is circular, a B-spline
  // ellipse is not.
  const occ::handle<Geom_BSplineCurve> half_circle =
      GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(new Geom_Circle(gp_Ax2(gp_Pnt(1, 1, 1), gp_Dir(0, 1, 0)), 2.5),
                                                             0.0, kPi));
  CHECK(is_circular(BRepBuilderAPI_MakeEdge(half_circle).Edge()));
  const occ::handle<Geom_BSplineCurve> oval_arc = GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(base_ellipse, 0.0, kPi));
  CHECK(!is_circular(BRepBuilderAPI_MakeEdge(oval_arc).Edge()));
}

// The faces and edges by index as the `faces` and `edges` queries read
// those stored as other types (mitcad#1863): as face_geometry and
// edge_geometry read one, while face_geometries keeps the stored type.
void test_recognised_geometries_by_index() {
  const gp_Ax3 frame(gp_Pnt(1, 2, 3), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0));
  const Shape wall =
      named_face(spline_face(new Geom_CylindricalSurface(frame, 4.0), 0.0, 2.0 * kPi, 0.0, 2.0), "W");
  CHECK(face_geometries(wall).at(0).type == "bspline");
  // Asked twice: the second answer as the first (the kind kept by the face).
  for (int pass = 0; pass < 2; ++pass) {
    const std::vector<SurfaceDescription> faces = recognised_face_geometries(wall, {0});
    CHECK(faces.size() == 1);
    if (faces.size() == 1) {
      CHECK(faces[0].type == "cylinder");
      CHECK(std::abs(faces[0].radius - 4.0) < 1e-6);
      CHECK(along(faces[0].axis, 0, 0, 1));
      CHECK(faces[0].origin.Distance(gp_Pnt(1, 2, 4)) < 1e-6);
    }
  }
  std::vector<int> all_edges;
  for (int i = 0; i < wall.edge_count(); ++i) {
    all_edges.push_back(i);
  }
  const std::vector<CurveDescription> edges = recognised_edge_geometries(wall, all_edges);
  CHECK(edges.size() == all_edges.size());
  int circles = 0;
  for (const CurveDescription& edge : edges) {
    if (edge.type == "circle") {
      ++circles;
      CHECK(std::abs(edge.radius - 4.0) < 1e-6);
    }
  }
  CHECK(circles == 2);
  CHECK(throws_with([&wall] { recognised_face_geometries(wall, {1}); }, "past the body's faces"));
  CHECK(throws_with([&wall] { recognised_edge_geometries(wall, {-1}); }, "past the body's edges"));

  // A wavy B-spline keeps its type, the second time too.
  NCollection_Array2<gp_Pnt> poles(1, 4, 1, 4);
  for (int i = 1; i <= 4; ++i) {
    for (int j = 1; j <= 4; ++j) {
      poles(i, j) = gp_Pnt(i * 10.0, j * 10.0, ((i + 2 * j) % 3) * 4.0);
    }
  }
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  NCollection_Array1<int> multiplicities(1, 2);
  multiplicities(1) = 4;
  multiplicities(2) = 4;
  const Shape wavy = named_face(
      face_on(new Geom_BSplineSurface(poles, knots, knots, multiplicities, multiplicities, 3, 3), 0.0, 1.0, 0.0, 1.0),
      "S");
  for (int pass = 0; pass < 2; ++pass) {
    const std::vector<SurfaceDescription> faces = recognised_face_geometries(wavy, {0});
    CHECK(faces.size() == 1 && faces[0].type == "bspline");
  }
}

} // namespace

void datum_tests() {
  test_surfaces();
  test_split_faces_on_one_plane();
  test_curves_and_vertices();
  test_paths();
  test_face_points_and_display();
  test_round_faces_stored_as_other_surfaces();
  test_recognised_geometries_by_index();
}

} // namespace test
