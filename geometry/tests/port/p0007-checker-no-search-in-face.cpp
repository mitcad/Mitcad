// SPDX-License-Identifier: MIT
// Port patch 0007: BRepCheck_Analyzer checks each vertex and each edge of a
// face in the context of that face without searching the face for it first
// (BRepCheck_Vertex::InContextOfAncestor, BRepCheck_Edge::InContextOfAncestor):
// the search made the check quadratic in the edges of a face. The statuses
// stay as they were. The bound: a face of one wire of 40 000 edges (one
// wire, so the wire checks of patch 0008 do not count), checked without
// geometric controls. Searching the face for each of its 40 000 edges and
// 80 000 vertex uses goes through some 3e9 sub-shapes, tens of seconds; with
// the patch the check takes about a second, so 8 s leaves room for a loaded
// machine. A face of a few edges is checked alike.

#include <chrono>
#include <cmath>
#include <cstdio>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pln.hxx>

#include "port_test.hpp"

namespace {

// A regular polygon of `sides` edges, radius 100, in the plane z = 0, as a
// face.
TopoDS_Face polygon_face(int sides) {
  BRepBuilderAPI_MakePolygon polygon;
  for (int k = 0; k < sides; ++k) {
    const double angle = 2.0 * test::kPi * k / sides;
    polygon.Add(gp_Pnt(100.0 * std::cos(angle), 100.0 * std::sin(angle), 0.0));
  }
  polygon.Close();
  return BRepBuilderAPI_MakeFace(gp_Pln(), polygon.Wire()).Face();
}

// A face of six edges: valid, as before.
void test_small_face_is_valid() {
  const TopoDS_Face face = polygon_face(6);
  CHECK(port::count_of(face, TopAbs_EDGE) == 6);
  CHECK(port::valid(face));
}

// The bound: a face of 40 000 edges is valid, and checked within 8 s.
void test_face_of_forty_thousand_edges_within_eight_seconds() {
  const TopoDS_Face face = polygon_face(40000);
  CHECK(port::count_of(face, TopAbs_EDGE) == 40000);
  const auto               start = std::chrono::steady_clock::now();
  const BRepCheck_Analyzer analyzer(face, false);
  const bool               valid   = analyzer.IsValid();
  const double             seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("topological check of a face of 40 000 edges: %.2f s\n", seconds);
  CHECK(valid);
  CHECK(seconds < 8.0);
}

} // namespace

MITCAD_PORT_TEST("0007", test_small_face_is_valid);
MITCAD_PORT_TEST("0007", test_face_of_forty_thousand_edges_within_eight_seconds);
