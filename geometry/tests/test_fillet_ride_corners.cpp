// SPDX-License-Identifier: MIT
// Roundings that ride on another rounding's contact line (port patch 0016:
// two roundings of a face narrower than both, no ball touching the faces
// beyond it on both sides) and the corners at their ends, built with OCCT's
// fillet alone (port patches 1640 to 1642, mitcad#1499, after mitcad#1494):
//
// - a plate 1.3 thick (25.2 by 20) rounded with 1: the rims of an end face
//   alone, with the vertical edges at both its ends (the end face consumed
//   between the two rides, corners at both ends), and all 12 edges (at each
//   top corner two riders and the vertical edge, cut to nothing between the
//   sphere of the bottom corner and the top one);
// - the roof slab of mitcad#1482 (a section of 7 points, extruded 25.2),
//   its end face's two edges along one slope (1.3 apart square to the
//   slopes) rounded with 1: the underside's rounding, the shorter one,
//   rides on the slope's contact line from the eave, where it ends on the
//   line's end on the eave's edge, to the ceiling's corner, where the line
//   runs on.
//
// Each result is one valid closed solid with no face of no area, its volume
// checked against the removed volume worked out from the roundings' own
// sections.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

// A check of |a - b| <= tol that prints both values when it fails.
void check_within(double a, double b, double tol, const char* what) {
  if (std::abs(a - b) > tol) {
    std::fprintf(stderr, "check failed: %s: %.6f != %.6f (tolerance %.6f)\n", what, a, b, tol);
    ++failures;
  } else {
    std::printf("  %s: %.6f (expected %.6f, tolerance %.6f)\n", what, a, b, tol);
  }
}

// One solid, every edge on two faces, no face of no area, valid.
void check_closed_solid(const TopoDS_Shape& shape) {
  CHECK(BRepCheck_Analyzer(shape).IsValid());
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
  }
  CHECK(solids == 1);
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    CHECK(BRep_Tool::Degenerated(TopoDS::Edge(faces.FindKey(i))) || faces(i).Extent() == 2);
  }
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    CHECK(area_of(face.Current()) > 1e-6);
  }
}

// A check of lo <= a <= hi that prints the values.
void check_between(double a, double lo, double hi, const char* what) {
  if (a < lo || a > hi) {
    std::fprintf(stderr, "check failed: %s: %.6f not in [%.6f, %.6f]\n", what, a, lo, hi);
    ++failures;
  } else {
    std::printf("  %s: %.6f (in [%.6f, %.6f])\n", what, a, lo, hi);
  }
}

// Whether a face of the shape lies in the plane where coordinate `axis` (0
// x, 1 y, 2 z) is c (all its vertices).
bool has_face_in_plane(const TopoDS_Shape& shape, int axis, double c) {
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    bool all = true;
    for (TopExp_Explorer v(face.Current(), TopAbs_VERTEX); v.More() && all; v.Next()) {
      all = std::abs(BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Coord(axis + 1) - c) < 1e-6;
    }
    if (all) {
      return true;
    }
  }
  return false;
}

// The section (y, z) extruded along +x by `length`.
TopoDS_Shape prism(const std::vector<gp_Pnt2d>& section, double length) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt2d& p : section) {
    polygon.Add(gp_Pnt(0.0, p.X(), p.Y()));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(length, 0.0, 0.0)).Shape();
}

// The edge from a to b (either way), or null.
TopoDS_Edge edge_between(const TopoDS_Shape& body, const gp_Pnt& a, const gp_Pnt& b) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    TopoDS_Vertex v1, v2;
    TopExp::Vertices(TopoDS::Edge(edges(i)), v1, v2);
    const gp_Pnt p1 = BRep_Tool::Pnt(v1), p2 = BRep_Tool::Pnt(v2);
    if ((p1.Distance(a) < 1e-6 && p2.Distance(b) < 1e-6) || (p1.Distance(b) < 1e-6 && p2.Distance(a) < 1e-6)) {
      return TopoDS::Edge(edges(i));
    }
  }
  return TopoDS_Edge();
}

// The edges rounded with r by OCCT's fillet: the result, or a null shape.
TopoDS_Shape rounded(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r) {
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    for (const TopoDS_Edge& edge : edges) {
      CHECK(!edge.IsNull());
      if (edge.IsNull()) {
        return TopoDS_Shape();
      }
      fillet.Add(r, edge);
    }
    fillet.Build();
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      return TopoDS_Shape();
    }
    return fillet.Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    CHECK(false);
  }
  return TopoDS_Shape();
}

// ---------------------------------------------------------------- the plate

constexpr double kLength = 25.2; // along x
constexpr double kWidth = 20.0;  // along y
constexpr double kThick = 1.3;   // along z
constexpr double kRound = 1.0;

TopoDS_Shape plate() {
  return prism({gp_Pnt2d(0, 0), gp_Pnt2d(kWidth, 0), gp_Pnt2d(kWidth, kThick), gp_Pnt2d(0, kThick)}, kLength);
}

// The section removed along a face's two rims, square to them, in (u, z): u
// the distance from the face. The first rim's rounding (its ball touching
// the face and the bottom, its contact on the face at z = r) is whole; the
// other one's ball touches the top and passes through that contact line, at
// d from the face. The removed part of the square [0, r] x [0, t] lies
// outside both arcs: built as a face and measured.
double rim_section() {
  const double r = kRound, t = kThick;
  const double d = std::sqrt(r * r - (r - (t - r)) * (r - (t - r)));
  // The kept part of the square: from (r, 0) round the first arc to the line
  // (0, r), the second arc to (d, t), the top to (r, t), down to (r, 0).
  const gp_Pnt a(r, 0, 0), line(0, 0, r), b(d, 0, t), c(r, 0, t);
  const gp_Pnt m1(r - r * std::sin(kPi / 4), 0, r - r * std::cos(kPi / 4));
  const gp_Pnt centre2(d, 0, t - r);
  gp_Vec v = gp_Vec(centre2, line).Normalized() + gp_Vec(centre2, b).Normalized();
  v.Normalize();
  const gp_Pnt m2 = centre2.Translated(v * r);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(a, m1, line).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(line, m2, b).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(b, c).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(c, a).Edge());
  const double kept = std::abs(area_of(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face()));
  return r * t - kept;
}

// The corner cells' removed volumes (the cell r by r by t at a vertical edge
// of the plate), as the union of the roundings' removal sets integrated on a
// grid of 1/160 (the riders' corners are no rolling ball's: their sharp
// intersections give these, OCCT's fill of the corner differs a little):
// - the rims of one face and the vertical edge, less what the rims take
//   alone: 0.14997;
// - the rims of both faces and the vertical edge, the bottom corner the
//   sphere of the three roundings below the line: 0.60279.
constexpr double kCornerOneFace = 0.14997;
constexpr double kCornerCell = 0.60279;

std::vector<TopoDS_Edge> plate_edges(const TopoDS_Shape& body, const std::vector<std::vector<double>>& ends) {
  std::vector<TopoDS_Edge> edges;
  for (const std::vector<double>& e : ends) {
    edges.push_back(edge_between(body, gp_Pnt(e[0], e[1], e[2]), gp_Pnt(e[3], e[4], e[5])));
  }
  return edges;
}

// The end face x = 0's rims: its rides consume it whole.
void test_plate_end_face_rims() {
  const TopoDS_Shape body = plate();
  const double v = volume_of(body);
  const TopoDS_Shape result =
    rounded(body, plate_edges(body, {{0, 0, 0, 0, kWidth, 0}, {0, 0, kThick, 0, kWidth, kThick}}), kRound);
  if (result.IsNull()) {
    return;
  }
  check_closed_solid(result);
  CHECK(!has_face_in_plane(result, 0, 0.0));
  check_within(v - volume_of(result), rim_section() * kWidth, 1e-5, "plate, end face's rims: removed");
}

// The end face's rims and the vertical edges at both its ends: the end face,
// between the two rides, which end in corners at both ends, is consumed.
void test_plate_end_face_rims_and_corners() {
  const TopoDS_Shape body = plate();
  const double v = volume_of(body);
  const TopoDS_Shape result = rounded(body,
                                      plate_edges(body,
                                                  {{0, 0, 0, 0, kWidth, 0},
                                                   {0, 0, kThick, 0, kWidth, kThick},
                                                   {0, 0, 0, 0, 0, kThick},
                                                   {0, kWidth, 0, 0, kWidth, kThick}}),
                                      kRound);
  if (result.IsNull()) {
    return;
  }
  check_closed_solid(result);
  CHECK(!has_face_in_plane(result, 0, 0.0));
  check_within(v - volume_of(result), rim_section() * kWidth + 2.0 * kCornerOneFace, 0.002,
               "plate, end face's rims and corners: removed");
}

// All 12 edges.
void test_plate_all_edges() {
  const TopoDS_Shape body = plate();
  const double v = volume_of(body);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> map;
  TopExp::MapShapes(body, TopAbs_EDGE, map);
  std::vector<TopoDS_Edge> edges;
  for (int i = 1; i <= map.Extent(); ++i) {
    edges.push_back(TopoDS::Edge(map(i)));
  }
  CHECK(edges.size() == 12);
  const TopoDS_Shape result = rounded(body, edges, kRound);
  if (result.IsNull()) {
    return;
  }
  check_closed_solid(result);
  // No side face left (each consumed by its rims' rides).
  CHECK(!has_face_in_plane(result, 0, 0.0));
  CHECK(!has_face_in_plane(result, 0, kLength));
  CHECK(!has_face_in_plane(result, 1, 0.0));
  CHECK(!has_face_in_plane(result, 1, kWidth));
  // The rims along each side between its corner cells, and the four cells.
  // OCCT fills each top corner, between its two riders' ends and the bottom
  // sphere's top, with one surface (G0), which takes a little more than the
  // sharp intersections of the three roundings (0.016 a corner): from those
  // up to 0.03 a corner more.
  const double rims = rim_section() * 2.0 * ((kLength - 2.0 * kRound) + (kWidth - 2.0 * kRound));
  const double sharp = rims + 4.0 * kCornerCell;
  check_between(v - volume_of(result), sharp - 0.002, sharp + 4.0 * 0.03, "plate, all edges: removed");
}

// ------------------------------------------------------------ the roof slab

// The slab's section (y, z) as in test_fillet_roof_slab.
std::vector<gp_Pnt2d> slab_section() {
  const double rise = 2.6 / 1.506;
  const double ridge = 17.8 + (10.9 + 14.906) * rise;
  const double ceiling = ridge - 2.6 - 3.0;
  const double ceiling_left = -13.4 + (ceiling - 17.8) / rise;
  return {gp_Pnt2d(-14.906, 17.8),         gp_Pnt2d(-13.4, 17.8), gp_Pnt2d(ceiling_left, ceiling),
          gp_Pnt2d(21.8 - ceiling_left, ceiling), gp_Pnt2d(35.2, 17.8), gp_Pnt2d(36.706, 17.8),
          gp_Pnt2d(10.9, ridge)};
}

// The end face's edge along the left slope's top (from the eave's tip to the
// ridge) and along its underside (from the eave's inner edge to the ceiling).
TopoDS_Edge slab_edge(const TopoDS_Shape& body, int from, int to) {
  const std::vector<gp_Pnt2d> s = slab_section();
  return edge_between(body, gp_Pnt(0, s[from].X(), s[from].Y()), gp_Pnt(0, s[to].X(), s[to].Y()));
}

// The removed volumes as the union of the roundings' removal sets (the end
// face square to the slopes: 90 degree edges; the underside's ball touching
// the underside and passing through the slope's contact line on the end
// face; the ends cut by the solid, the underside's rounding at the ceiling's
// plane), integrated on a grid of 0.004: the slope's edge alone 10.98640,
// the underside's alone 9.65367, both 14.01883. OCCT's own corners at the
// ends cut each single rounding 0.007 off that, so the pair is checked by
// the volume both take together, 6.62124, which the ends hardly change.
constexpr double kSlabOverlap = 6.62124;

void test_slab_end_face_pair() {
  const TopoDS_Shape body = prism(slab_section(), 25.2);
  const double v = volume_of(body);
  const TopoDS_Edge top = slab_edge(body, 6, 0), under = slab_edge(body, 1, 2);
  CHECK(!top.IsNull() && !under.IsNull());
  if (top.IsNull() || under.IsNull()) {
    return;
  }
  const TopoDS_Shape both = rounded(body, {under, top}, kRound);
  const TopoDS_Shape top_only = rounded(body, {top}, kRound);
  const TopoDS_Shape under_only = rounded(body, {under}, kRound);
  if (both.IsNull() || top_only.IsNull() || under_only.IsNull()) {
    return;
  }
  check_closed_solid(both);
  const double removed = v - volume_of(both);
  const double overlap = (v - volume_of(top_only)) + (v - volume_of(under_only)) - removed;
  check_within(removed, 14.01883, 0.02, "slab, end face pair: removed");
  check_within(overlap, kSlabOverlap, 0.01, "slab, end face pair: taken by both");
  // At 0.9 too.
  const TopoDS_Shape both09 = rounded(body, {under, top}, 0.9);
  if (!both09.IsNull()) {
    check_closed_solid(both09);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_plate_end_face_rims", test::test_plate_end_face_rims);
  test::guarded("test_plate_end_face_rims_and_corners", test::test_plate_end_face_rims_and_corners);
  test::guarded("test_plate_all_edges", test::test_plate_all_edges);
  test::guarded("test_slab_end_face_pair", test::test_slab_end_face_pair);
  if (test::failures == 0) {
    std::puts("test_fillet_ride_corners: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
