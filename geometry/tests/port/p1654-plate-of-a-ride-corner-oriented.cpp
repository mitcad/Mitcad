// SPDX-License-Identifier: MIT
// Port patch 1654: the plate that fills a corner with a rider
// (PerformMoreThreeCorner, G0 constraints, 1640) takes the orientation of
// its curves from the first stripe at the corner that rides on no other's
// contact line, not from the corner's first stripe whatever it is.
//
// A gable roof's slab (as in geometry/tests/test_fillet_roof_slab.cpp): a
// section of 7 points on the yz plane extruded 25.2 along x, its slopes
// 1.3 thick, each eave a level face 1.506 wide between the acute edge at
// its tip (60 degrees) and the obtuse one where the underside begins (120
// degrees). All 21 edges rounded with r, added in the body's own edge
// order (TopExp::MapShapes, the order the MitCad facade lists them in).
// From r = 0.652 on the roundings of an eave's two edges reach farther
// across it than it is wide (r / tan(30) + r / tan(60) = 2.309 r > 1.506),
// and from r = 0.65 those of each slope's two edges across the end faces
// (2 r > 1.3): the obtuse edge's rounding rides on the tip's contact line
// (0016, the one whose contact lies nearer its edge since 1658) and the
// underside's end edge on the slope's line, and the eave's end edge meets
// both. At r = 0.8 the corner at the left eave's end has the obtuse edge's
// rider as its first stripe: its side on the line has its transition from
// the line's support (RideOnOverlap, 1640), and without the patch the
// plate's p-curves were oriented from it, its face's wire the wrong way
// round (the face's area -0.34). The check: a valid result whose every face
// has a positive area.
//
// Why the slab's own test (geometry.fillet_roof_slab) does not show it: it
// rounds the slab through the MitCad facade, which repairs a result that
// BRepCheck rejects by shape healing (ShapeFix can turn a face's wire that
// runs the wrong way round) and accepts the repair when its volume stays
// near the body's, and which has further routes (a 0.1 % smaller retry,
// corners built apart); this test calls OCCT's fillet alone, once.
//
// The boundary: r = 0.5, where no rounding rides (1.155 of the eave's
// 1.506, 1.0 of the end faces' 1.3): no corner with a rider, valid either
// way.

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

// The section's points (y, z), counter-clockwise: the slopes rise
// 2.6 / 1.506 from the eaves' tips, the underside runs parallel to them
// 2.6 lower (1.3 square to them), and the ceiling is level 3 below where
// the underside's lines would meet.
std::vector<gp_Pnt2d> section() {
  const double rise         = 2.6 / 1.506;
  const double ridge        = 17.8 + (10.9 + 14.906) * rise;
  const double ceiling      = ridge - 2.6 - 3.0;
  const double ceiling_left = -13.4 + (ceiling - 17.8) / rise;
  return {gp_Pnt2d(-14.906, 17.8),
          gp_Pnt2d(-13.4, 17.8),
          gp_Pnt2d(ceiling_left, ceiling),
          gp_Pnt2d(21.8 - ceiling_left, ceiling),
          gp_Pnt2d(35.2, 17.8),
          gp_Pnt2d(36.706, 17.8),
          gp_Pnt2d(10.9, ridge)};
}

// The section on the plane x = 0 (its normal +x, its axes y and z, as the
// facade's sketch frame has them) extruded 25.2 along +x.
TopoDS_Shape slab() {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt2d& p : section()) {
    polygon.Add(gp_Pnt(0.0, p.X(), p.Y()));
  }
  polygon.Close();
  const gp_Pln       plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), gp_Dir(0, 1, 0)));
  const TopoDS_Face  face = BRepBuilderAPI_MakeFace(plane, polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(25.2, 0.0, 0.0)).Shape();
}

// All the slab's edges rounded with r in the body's edge order: valid, and
// no face whose wire runs the wrong way round (a negative area).
void check_all_edges(double r) {
  std::printf("the roof slab's 21 edges rounded with %.2f\n", r);
  const TopoDS_Shape body = slab();
  CHECK(port::valid(body));
  CHECK(port::count_of(body, TopAbs_FACE) == 9);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(body, TopAbs_EDGE, all);
  std::vector<TopoDS_Edge> edges;
  for (int i = 1; i <= all.Extent(); ++i) {
    edges.push_back(TopoDS::Edge(all(i)));
  }
  CHECK(edges.size() == 21);
  const TopoDS_Shape result = port::rounded(body, edges, r);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  int    wrong    = 0;
  double smallest = 1e100;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const double area = port::area_of(f.Current());
    smallest          = std::min(smallest, area);
    wrong += area > 0.0 ? 0 : 1;
  }
  std::printf("  %d faces, the smallest area %.6f, %d of no or negative area\n",
              port::count_of(result, TopAbs_FACE), smallest, wrong);
  CHECK(wrong == 0);
  CHECK(port::volume_of(result) > 0.0);
  CHECK(port::volume_of(result) < port::volume_of(body));
}

// The eaves' and the end faces' rides, a corner with a rider at the left
// eave's ends.
void test_plate_of_a_corner_with_a_rider() {
  check_all_edges(0.8);
}

// The boundary: no rides, no corner with a rider.
void test_corners_without_riders() {
  check_all_edges(0.5);
}

} // namespace

MITCAD_PORT_TEST("1654", test_plate_of_a_corner_with_a_rider);
MITCAD_PORT_TEST("1654", test_corners_without_riders);
