// SPDX-License-Identifier: MIT
// Port patch 2229: a chamfer whose cap on the face at its edge's end leaves
// that face across an edge tangent to the face beyond and comes back
// (GitHub mitcad/Mitcad#566, mitcad#2229).
//
// The body: a block stepped in its section (x 0..10; y 0..5 up to z = 5,
// y 5..10 up to z = 10) rounded with 5 on its sides' and steps' edges and
// the step's inner edge, by MitCad's rounding (the block's core swept by
// the ball, mitcad#1954 and mitcad#2068: OCCT's own fillet does not build
// it, and OCCT has no sweep of a region by a ball; the chamfer below is
// OCCT's alone). The back's bottom edge (y = 10, z = 0) ends at x = 0 and
// x = 10 on a flat face (the core's region: 5 from the edge along the bottom
// and the back, 2.89 along the diagonal) that ends in a torus band tangent
// to it. A chamfer of 4.5 or 4.9 on that edge has its cap on the flat face
// leave it across the torus band's edge and come back.
//
// Before the patch the whole cap was stored on the flat face: the face
// trimmed by a line running outside it (area -0.69 at 4.5, its wire crossing
// itself), the torus band left whole under the bevel, the shape invalid and
// 0.0036 mm3 smaller than the exact one (the probe of mitcad#2229,
// geometry/tests/probe_end_face_dressup.cpp). Now the cap is cut by the
// torus band between the crossings: one valid solid of the exact volume
// (the body less its common part with the bevel's wedge), the flat faces
// kept in their two corner pieces (positive areas, as in the exact shape).
//
// The nearest case outside the patch: the chamfer of 4, whose cap stays on
// the flat face: valid and exact as before.
//
// The rounding and the bevel of exactly 10 (mitcad#2329): as wide as the
// bottom and the back, their contacts touching each at one point; the
// rounding failed before (no domain, no start). Now one valid solid of the
// exact volume, no face under 0.01 mm2.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"
#include "port_test.hpp"

namespace {

using namespace mitcad::geometry;

// A rectangle extruded from z = 0 to `end`, its lines named c<first> to
// c<first + 3> as a sketch names them.
ShapePtr extruded(const std::string& feature, int first, double x, double y, double width, double height,
                  double end) {
  ExtrudeSpec spec;
  spec.feature = feature;
  Region region = rectangle_region(x, y, width, height);
  std::string key = "r{";
  for (int i = 0; i < 4; ++i) {
    const auto line = [first](int k) { return "c" + std::to_string(first + (k + 4) % 4); };
    region.loops[0].segments[static_cast<std::size_t>(i)].name = line(i) + "[" + line(i - 1) + "," + line(i + 1) + "]";
    key += (i == 0 ? "" : ",") + region.loops[0].segments[static_cast<std::size_t>(i)].name;
  }
  region.name = key + "}";
  spec.regions = {std::move(region)};
  spec.start = 0.0;
  spec.end = end;
  return extrude(spec);
}

// The name of the edge whose middle is at p (empty when none is).
std::string edge_named_at(const Shape& shape, const gp_Pnt& p) {
  for (int i = 0; i < shape.edge_count(); ++i) {
    if (BRep_Tool::Degenerated(shape.edge(i))) {
      continue;
    }
    const BRepAdaptor_Curve curve(shape.edge(i));
    if (curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p) < 1e-6) {
      return shape.edge_name(i);
    }
  }
  return {};
}

// The stepped block rounded by 5 on its eleven edges.
TopoDS_Shape rounded_block() {
  const ShapePtr low = extruded("F1", 1, 0, 0, 10, 5, 5);
  const ShapePtr high = extruded("F2", 5, 0, 5, 10, 5, 10);
  const ShapePtr block = boolean(BooleanOp::Join, {low.get()}, *high).pieces.at(0).shape;
  FilletSet round;
  for (const gp_Pnt& m : {gp_Pnt(0, 0, 2.5), gp_Pnt(0, 2.5, 5), gp_Pnt(0, 5, 7.5), gp_Pnt(0, 7.5, 10),
                          gp_Pnt(10, 0, 2.5), gp_Pnt(10, 2.5, 5), gp_Pnt(10, 5, 7.5), gp_Pnt(10, 7.5, 10),
                          gp_Pnt(5, 0, 5), gp_Pnt(5, 5, 10), gp_Pnt(5, 5, 5)}) {
    round.edges.push_back(edge_named_at(*block, m));
  }
  round.radius = 5;
  return fillet("F3", *block, {round})->occt();
}

// The body less its common part with the bevel's wedge (the triangle
// y - (10 - d) > z), or with the rounding's corner (`round`: the square of
// d less the circle about y = 10 - d, z = d), along x past the body.
TopoDS_Shape exact(const TopoDS_Shape& body, double d, bool round = false) {
  TopoDS_Shape tool;
  if (round) {
    const TopoDS_Shape square = BRepPrimAPI_MakeBox(gp_Pnt(-5, 10 - d, 0), gp_Pnt(15, 10, d)).Shape();
    const TopoDS_Shape ball =
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-5, 10 - d, d), gp_Dir(1, 0, 0)), d, 20).Shape();
    tool = BRepAlgoAPI_Cut(square, ball).Shape();
  } else {
    BRepBuilderAPI_MakePolygon triangle(gp_Pnt(-5, 10 - d, 0), gp_Pnt(-5, 10, 0), gp_Pnt(-5, 10, d), true);
    tool = BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(triangle.Wire()).Face(), gp_Vec(20, 0, 0)).Shape();
  }
  return BRepAlgoAPI_Cut(BRepBuilderAPI_Copy(body).Shape(), tool).Shape();
}

// The area of the faces on the plane x = `x` (signed: a face whose wire
// crosses itself counts less).
double area_on_plane(const TopoDS_Shape& shape, double x) {
  double area = 0.0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()), false);
    if (surface.GetType() == GeomAbs_Plane && std::abs(std::abs(surface.Plane().Axis().Direction().X()) - 1.0) < 1e-9 &&
        std::abs(surface.Plane().Location().X() - x) < 1e-7) {
      area += port::area_of(ex.Current());
    }
  }
  return area;
}

// The length of the edges of the faces on the plane x = `x`.
double boundary_on_plane(const TopoDS_Shape& shape, double x) {
  double length = 0.0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()), false);
    if (surface.GetType() == GeomAbs_Plane && std::abs(std::abs(surface.Plane().Axis().Direction().X()) - 1.0) < 1e-9 &&
        std::abs(surface.Plane().Location().X() - x) < 1e-7) {
      GProp_GProps props;
      BRepGProp::LinearProperties(ex.Current(), props);
      length += props.Mass();
    }
  }
  return length;
}

// The smallest face's area.
double smallest_face(const TopoDS_Shape& shape) {
  double smallest = 1e300;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    smallest = std::min(smallest, port::area_of(ex.Current()));
  }
  return smallest;
}

// The back's bottom edge bevelled (or, `round`, rounded) by d with OCCT
// alone: one valid solid of the exact shape's volume (within `relative`),
// the end faces of its areas.
void check_bevel(const TopoDS_Shape& body, double d, bool round = false, double relative = 1e-7) {
  const std::vector<TopoDS_Edge> edge = port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.X() - 5) < 1e-7 && std::abs(p.Y() - 10) < 1e-7 && std::abs(p.Z()) < 1e-7;
  });
  CHECK(edge.size() == 1);
  if (edge.size() != 1) {
    return;
  }
  const TopoDS_Shape result = round ? port::rounded(body, edge, d) : port::chamfered(body, edge, d);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  const TopoDS_Shape reference = exact(body, d, round);
  const double volume = port::volume_of(result);
  const double expected = port::volume_of(reference);
  std::printf("%s %g: valid %d, solids %d, volume %.9f (exact %.9f), end faces %.6f and %.6f (exact %.6f "
              "and %.6f)\n",
              round ? "fillet" : "chamfer", d, port::valid(result) ? 1 : 0, port::count_of(result, TopAbs_SOLID),
              volume, expected,
              area_on_plane(result, 0.0), area_on_plane(result, 10.0), area_on_plane(reference, 0.0),
              area_on_plane(reference, 10.0));
  // At least the baseline 1e-6 mm (the end cuts on the balls and the torus
  // band are approximated, and their rounding differs by platform: arm64
  // fuses multiply-adds): the volume within 1e-6 mm over the result's
  // surface, an end face's area within 1e-6 mm along both shapes' edges on
  // that plane. Both stay orders of magnitude below the patch's faults
  // (0.0036 mm3 and 0.058 mm3 short, end faces of area -0.69 and -3.06).
  const double volume_tolerance = std::max(relative * expected, 1e-6 * port::area_of(result));
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  CHECK(std::abs(volume - expected) <= volume_tolerance);
  if (std::abs(volume - expected) > volume_tolerance) {
    std::printf("volume off by %.3g (tolerance %.3g)\n", volume - expected, volume_tolerance);
  }
  for (const double x : {0.0, 10.0}) {
    const double area_off = area_on_plane(result, x) - area_on_plane(reference, x);
    const double area_tolerance =
        std::max(1e-6, 1e-6 * (boundary_on_plane(result, x) + boundary_on_plane(reference, x)));
    CHECK(std::abs(area_off) <= area_tolerance);
    if (std::abs(area_off) > area_tolerance) {
      std::printf("end face at x = %g: area off by %.3g (tolerance %.3g)\n", x, area_off, area_tolerance);
    }
  }
}

// The reported case: the cap leaves the flat end face and comes back.
void test_cap_leaving_the_end_face_across_a_tangent_edge() {
  const TopoDS_Shape body = rounded_block();
  for (const double d : {4.5, 4.9}) {
    check_bevel(body, d);
  }
}

// The chamfer of 5: its contacts reach the flat end face's corners, its
// cap is the chord of the torus band's edge, wholly outside the face: the
// cap on the band alone, the end face gone (before: valid but 0.058 mm3
// short, the end face trimmed by the chord, area -3.06).
void test_cap_wholly_across_a_tangent_edge() {
  check_bevel(rounded_block(), 5.0);
}

// The chamfers of 5.5 and 7: the bevel's contacts leave the bottom and the
// back onto the half balls before the edge's ends, and the end is cut by a
// half ball, the torus band and the other half ball. GeomInt's line on a
// ball is the short arc between the ends, found the other way round at the
// spine's first end and trimmed round the circle's period, the long way
// (the bevel face's and the balls' wires crossing themselves, their areas
// negative; ChFi3d_ComputeCurves).
void test_bevel_end_cut_by_the_half_balls() {
  const TopoDS_Shape body = rounded_block();
  for (const double d : {5.5, 7.0}) {
    check_bevel(body, d);
  }
}

// The rounding of 6: its end crosses a half ball, the torus band, the flat
// end face (a lens between the band's edge and the rounding's end arc), the
// band again and the other half ball. Both visits of the band's edge took
// the same crossing, the end face went and the band's cut ran across the
// lens (four edges in one face, the volume 0; PerformIntersectionAtEnd).
// The rounding of 7, whose end crosses no end face, as before.
void test_rounding_end_across_the_end_faces_lens() {
  const TopoDS_Shape body = rounded_block();
  check_bevel(body, 6.0, true);
  check_bevel(body, 7.0, true);
}

// The rounding and the bevel of exactly 10, the width of the bottom and the
// back (mitcad#2329): the analytic blend's contacts touch each face at one
// point, where the face's half-ball edge reaches farthest from the edge,
// and the faces go whole. The hatching found no domain on either face and
// the walk no start: the rounding failed (MitCad built it 0.1 % smaller,
// with a sliver). Now the touch is a piece of no length, ended at both
// ends by the end cuts: one valid solid of the exact volume (within 1e-6:
// the end cuts on the balls and the band are approximated), no face under
// 0.01 mm2, no end face left.
void test_rounding_as_wide_as_both_faces() {
  const TopoDS_Shape body = rounded_block();
  for (const bool round : {true, false}) {
    const std::vector<TopoDS_Edge> edge = port::edges_where(body, [](const gp_Pnt& p) {
      return std::abs(p.X() - 5) < 1e-7 && std::abs(p.Y() - 10) < 1e-7 && std::abs(p.Z()) < 1e-7;
    });
    check_bevel(body, 10.0, round, 1e-6);
    if (edge.size() != 1) {
      continue;
    }
    const TopoDS_Shape result = round ? port::rounded(body, edge, 10.0) : port::chamfered(body, edge, 10.0);
    CHECK(!result.IsNull());
    if (!result.IsNull()) {
      std::printf("%s 10: smallest face %.6f\n", round ? "fillet" : "chamfer", smallest_face(result));
      CHECK(smallest_face(result) > 0.01);
    }
  }
}

// The nearest case outside it: the cap stays on the end face.
void test_cap_within_the_end_face() {
  check_bevel(rounded_block(), 4.0);
}

} // namespace

MITCAD_PORT_TEST("2229", test_cap_leaving_the_end_face_across_a_tangent_edge);
MITCAD_PORT_TEST("2229", test_cap_wholly_across_a_tangent_edge);
MITCAD_PORT_TEST("2229", test_bevel_end_cut_by_the_half_balls);
MITCAD_PORT_TEST("2229", test_rounding_end_across_the_end_faces_lens);
MITCAD_PORT_TEST("2229", test_cap_within_the_end_face);
MITCAD_PORT_TEST("2229", test_rounding_as_wide_as_both_faces);
