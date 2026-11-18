// SPDX-License-Identifier: MIT
// Port patch 0801: ChFi3d::IsTangentFaces takes two faces for tangent along
// an edge when the edge's curves on them lie within max(0.001, 7 x the
// edge's tolerance) of each other; stock OCCT allowed 1.5 x. Two coplanar
// faces and an edge on their common line whose curve on the second face is
// moved aside by a given distance: tangent up to seven tolerances apart,
// not beyond, as before within 1.5 tolerances, and the floor of 0.001 still
// decides for an edge of a small tolerance.

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRep_Builder.hxx>
#include <ChFi3d.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_Line.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Dir.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "port_test.hpp"

namespace {

constexpr double kLength = 10.0;

// Two faces of the plane z = 0, each on a plane of its own, meeting along
// the line x = 0 for 0 <= y <= 10: x from -1 to 0 and from 0 to 1. The
// planes' parameters are x and y.
struct Faces {
  TopoDS_Face left;
  TopoDS_Face right;
};

Faces coplanar_faces() {
  return {BRepBuilderAPI_MakeFace(gp_Pln(), -1.0, 0.0, 0.0, kLength).Face(),
          BRepBuilderAPI_MakeFace(gp_Pln(), 0.0, 1.0, 0.0, kLength).Face()};
}

// An edge of the given tolerance on the faces' common line, its curve on the
// left face on that line and its curve on the right face `apart` away from
// it (along x): the two curves lie `apart` from each other in space.
TopoDS_Edge edge_between(const Faces& faces, double tolerance, double apart) {
  BRep_Builder                  builder;
  TopoDS_Edge                   edge;
  const occ::handle<Geom_Curve> line(new Geom_Line(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)));
  builder.MakeEdge(edge, line, tolerance);
  TopoDS_Vertex first, last;
  builder.MakeVertex(first, gp_Pnt(0, 0, 0), tolerance);
  builder.MakeVertex(last, gp_Pnt(0, kLength, 0), tolerance);
  builder.Add(edge, first.Oriented(TopAbs_FORWARD));
  builder.Add(edge, last.Oriented(TopAbs_REVERSED));
  const occ::handle<Geom2d_Curve> on_left(new Geom2d_Line(gp_Pnt2d(0, 0), gp_Dir2d(0, 1)));
  const occ::handle<Geom2d_Curve> on_right(new Geom2d_Line(gp_Pnt2d(apart, 0), gp_Dir2d(0, 1)));
  builder.UpdateEdge(edge, on_left, faces.left, tolerance);
  builder.UpdateEdge(edge, on_right, faces.right, tolerance);
  builder.Range(edge, 0.0, kLength);
  return edge;
}

bool tangent_with_curves_apart(double tolerance, double apart) {
  const Faces       faces = coplanar_faces();
  const TopoDS_Edge edge  = edge_between(faces, tolerance, apart);
  CHECK_NEAR(BRep_Tool::Tolerance(edge), tolerance);
  return ChFi3d::IsTangentFaces(edge, faces.left, faces.right);
}

constexpr double kTolerance = 0.002;

// 6.5 tolerances apart (0.013): within the patch's 7 (0.014). Stock OCCT
// compared within 1.5 tolerances (0.003) and took the faces for faces
// meeting at an angle.
void test_curves_six_and_a_half_tolerances_apart_are_tangent() {
  CHECK(tangent_with_curves_apart(kTolerance, 6.5 * kTolerance));
}

// 7.5 tolerances apart (0.015): beyond the limit, not tangent.
void test_curves_seven_and_a_half_tolerances_apart_are_not_tangent() {
  CHECK(!tangent_with_curves_apart(kTolerance, 7.5 * kTolerance));
}

// 1.4 tolerances apart (0.0028): tangent, as before the patch.
void test_curves_one_and_a_half_tolerances_apart_are_tangent() {
  CHECK(tangent_with_curves_apart(kTolerance, 1.4 * kTolerance));
  CHECK(tangent_with_curves_apart(kTolerance, 0.0));
}

// An edge of tolerance 1e-5: seven tolerances are 7e-5, so the floor of
// 0.001 decides, as before: 0.0009 apart tangent, 0.0011 apart not.
void test_floor_of_a_thousandth_decides_for_a_tight_edge() {
  CHECK(tangent_with_curves_apart(1e-5, 0.0009));
  CHECK(!tangent_with_curves_apart(1e-5, 0.0011));
}

} // namespace

MITCAD_PORT_TEST("0801", test_curves_six_and_a_half_tolerances_apart_are_tangent);
MITCAD_PORT_TEST("0801", test_curves_seven_and_a_half_tolerances_apart_are_not_tangent);
MITCAD_PORT_TEST("0801", test_curves_one_and_a_half_tolerances_apart_are_tangent);
MITCAD_PORT_TEST("0801", test_floor_of_a_thousandth_decides_for_a_tight_edge);
