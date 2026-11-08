// SPDX-License-Identifier: MIT
// The roof slab's eave (mitcad#1500, after mitcad#1494 and mitcad#1499),
// built with OCCT's fillet alone (port patches 1650 to 1653). The slab of
// mitcad#1482 (a section of 7 points, extruded 25.2 along x): its eave is a
// level face 1.506 wide between the tip's edge (60 degrees) and the obtuse
// edge where the underside begins (120 degrees). Rounded with 0.9 or 1, the
// tip's ball touches the eave 1.73 r from the tip, past the obtuse edge
// (from r = 0.87): the tip's rounding rolls on the obtuse edge and consumes
// the eave.
//
// - The eave's two long edges: the obtuse edge's rounding, riding on the
//   tip's contact line, which is its own edge, takes nothing away (1650).
// - With the eave's end edges (90 degrees, between the eave and the end
//   faces): each meets the tip's blend first where its contact on the eave
//   reaches the obtuse edge (1651).
// - An end face's two edges along a slope and the eave's end edge: the
//   strip along the slope vanishes between the two rides (0016, 1640), and
//   the end edge's contact on the end face is cut down to one point by the
//   corners at both its ends (1652).
// - The tip's edge, the eave's end edge and the slope's top edge on the end
//   face (mitcad#1530): at the tip the three blends cut each other and meet
//   at one point (1653).
//
// Each result is one valid closed solid with no face of no area.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
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

constexpr double kLength = 25.2;       // along x
constexpr double kRise = 2.6 / 1.506;  // the slopes' dz / dy
constexpr double kEave = 17.8;         // the eave's height
constexpr double kTip = -14.906;       // the eave's tip (y)
constexpr double kInner = -13.4;       // the eave's obtuse edge (y)

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

// A check of |a - b| <= tol that prints both values.
void check_within(double a, double b, double tol, const char* what) {
  if (std::abs(a - b) > tol) {
    std::fprintf(stderr, "check failed: %s: %.6f != %.6f (tolerance %.6f)\n", what, a, b, tol);
    ++failures;
  } else {
    std::printf("  %s: %.6f (expected %.6f, tolerance %.6f)\n", what, a, b, tol);
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

// Whether a face of the shape lies on the left eave (all its vertices at the
// eave's height, at y < 0; the right eave is the other one).
bool has_left_eave_face(const TopoDS_Shape& shape) {
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    bool all = true;
    for (TopExp_Explorer v(face.Current(), TopAbs_VERTEX); v.More() && all; v.Next()) {
      const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
      all = std::abs(p.Z() - kEave) < 1e-6 && p.Y() < 0.0;
    }
    if (all) {
      return true;
    }
  }
  return false;
}

// The slab's section (y, z) as in test_fillet_roof_slab.
std::vector<gp_Pnt2d> slab_section() {
  const double ridge = kEave + (10.9 - kTip) * kRise;
  const double ceiling = ridge - 2.6 - 3.0;
  const double ceiling_left = kInner + (ceiling - kEave) / kRise;
  return {gp_Pnt2d(kTip, kEave),         gp_Pnt2d(kInner, kEave), gp_Pnt2d(ceiling_left, ceiling),
          gp_Pnt2d(21.8 - ceiling_left, ceiling), gp_Pnt2d(35.2, kEave), gp_Pnt2d(36.706, kEave),
          gp_Pnt2d(10.9, ridge)};
}

TopoDS_Shape slab() {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt2d& p : slab_section()) {
    polygon.Add(gp_Pnt(0.0, p.X(), p.Y()));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(kLength, 0.0, 0.0)).Shape();
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

// The edge between section points i and j at x (an end face's edge), or
// the edge along x at section point i (j < 0).
TopoDS_Edge slab_edge(const TopoDS_Shape& body, int i, int j, double x) {
  const std::vector<gp_Pnt2d> s = slab_section();
  if (j < 0) {
    return edge_between(body, gp_Pnt(0, s[i].X(), s[i].Y()), gp_Pnt(kLength, s[i].X(), s[i].Y()));
  }
  return edge_between(body, gp_Pnt(x, s[i].X(), s[i].Y()), gp_Pnt(x, s[j].X(), s[j].Y()));
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

// The tip's rounding as it rolls on the obtuse edge, in the section (y, z):
// its ball touches the top slope (from the tip T at 59.9 degrees) and
// passes through the obtuse edge O. It takes the triangle of T, its contact
// P on the slope and O, less the ball's segment over the chord P O (the arc
// from P to O stays inside the triangle).
struct TipRounding {
  double cy = 0, cz = 0; // the ball's centre
  double area = 0;       // the section it takes
};

TipRounding tip_rounding(double r) {
  const double dl = std::hypot(1.506, 2.6);
  const double dy = 1.506 / dl, dz = 2.6 / dl; // along the slope, up
  const double ny = dz, nz = -dy;              // from the top slope towards the underside
  const double oy = kInner - kTip, oz = 0.0;
  const double along = oy * dy + oz * dz, across = oy * ny + oz * nz; // O from T (across: 1.3)
  const double s = along + std::sqrt(r * r - (r - across) * (r - across));
  TipRounding t;
  t.cy = kTip + s * dy + r * ny;
  t.cz = kEave + s * dz + r * nz;
  const double py = kTip + s * dy, pz = kEave + s * dz;
  const double triangle = 0.5 * std::abs((py - kTip) * 0.0 - (pz - kEave) * (kInner - kTip));
  const double chord = std::hypot(py - kInner, pz - kEave);
  const double angle = 2.0 * std::asin(chord / (2.0 * r));
  t.area = triangle - 0.5 * r * r * (angle - std::sin(angle));
  return t;
}

// What an eave's end edge's rounding takes outside the tip's: its removal
// set {x < r, z < 17.8 + r, outside the tube} (90 degrees between the eave
// and the end face; at the obtuse edge's end it runs on to the underside,
// which cuts it) less the tip's, integrated over z (Simpson): at height z
// its depth in x times the section's width between the tip's arc and the
// underside.
double end_rounding_outside_tip(double r, const TipRounding& t) {
  const auto f = [&](double z) {
    const double under = kInner + (z - kEave) / kRise;
    const double arc = t.cy - std::sqrt(std::max(0.0, r * r - (z - t.cz) * (z - t.cz)));
    const double width = std::max(0.0, under - arc);
    const double h = kEave + r - z;
    return width * (r - std::sqrt(std::max(0.0, r * r - h * h)));
  };
  const int n = 20000;
  const double step = r / n;
  double sum = f(kEave) + f(kEave + r);
  for (int i = 1; i < n; ++i) {
    sum += (i % 2 == 1 ? 4.0 : 2.0) * f(kEave + i * step);
  }
  return sum * step / 3.0;
}

// The eave's two long edges: the tip's rounding alone takes material, along
// the whole length; the eave goes.
void test_eave_long_edges() {
  const TopoDS_Shape body = slab();
  const double v = volume_of(body);
  for (const double r : {0.9, 1.0}) {
    std::printf(" radius %.1f\n", r);
    const TopoDS_Shape result = rounded(body, {slab_edge(body, 0, -1, 0), slab_edge(body, 1, -1, 0)}, r);
    if (result.IsNull()) {
      continue;
    }
    check_closed_solid(result);
    CHECK(!has_left_eave_face(result));
    check_within(v - volume_of(result), tip_rounding(r).area * kLength, 1e-5, "eave's long edges: removed");
  }
}

// The eave's four edges: its end edges' roundings meet the tip's where their
// contacts on the eave reach the obtuse edge, and run on to the underside.
void test_eave_all_edges() {
  const TopoDS_Shape body = slab();
  const double v = volume_of(body);
  for (const double r : {0.9, 1.0}) {
    std::printf(" radius %.1f\n", r);
    const TopoDS_Shape result = rounded(body,
                                        {slab_edge(body, 0, -1, 0), slab_edge(body, 1, -1, 0),
                                         slab_edge(body, 0, 1, 0.0), slab_edge(body, 0, 1, kLength)},
                                        r);
    if (result.IsNull()) {
      continue;
    }
    check_closed_solid(result);
    CHECK(!has_left_eave_face(result));
    const TipRounding t = tip_rounding(r);
    check_within(v - volume_of(result), t.area * kLength + 2.0 * end_rounding_outside_tip(r, t), 1e-4,
                 "eave's four edges: removed");
  }
}

// An end face's two edges along the left slope and the eave's end edge: the
// end edge's contact on the end face is cut by both corners where it
// crosses the line both slope roundings ride on. The union of the
// roundings' removal sets takes at least what each takes and at most their
// sum (OCCT's corners hardly change that).
void test_end_face_strip_and_eave_end() {
  const TopoDS_Shape body = slab();
  const double v = volume_of(body);
  for (const double r : {0.7, 0.9, 1.0}) {
    std::printf(" radius %.1f\n", r);
    const TopoDS_Edge top = slab_edge(body, 6, 0, 0.0), under = slab_edge(body, 1, 2, 0.0);
    const TopoDS_Edge end = slab_edge(body, 0, 1, 0.0);
    const TopoDS_Shape all = rounded(body, {top, under, end}, r);
    const TopoDS_Shape pair = rounded(body, {top, under}, r);
    const TopoDS_Shape single = rounded(body, {end}, r);
    if (all.IsNull() || pair.IsNull() || single.IsNull()) {
      continue;
    }
    check_closed_solid(all);
    const double removed = v - volume_of(all), by_pair = v - volume_of(pair), by_end = v - volume_of(single);
    check_between(removed, std::max(by_pair, by_end), by_pair + by_end, "end face's strip and eave's end: removed");
  }
}

// Whether (y, z) lies inside the slab's section (even-odd rule).
bool in_section(const std::vector<gp_Pnt2d>& s, double y, double z) {
  bool in = false;
  for (std::size_t i = 0; i < s.size(); ++i) {
    const gp_Pnt2d& a = s[i];
    const gp_Pnt2d& b = s[(i + 1) % s.size()];
    if ((a.Y() > z) != (b.Y() > z) && y < a.X() + (z - a.Y()) * (b.X() - a.X()) / (b.Y() - a.Y())) {
      in = !in;
    }
  }
  return in;
}

// The tip's edge, the eave's end edge and the left slope's top edge on the
// end face x = 0 (port patch 1653): at the tip's vertex the three blends cut
// each other pairwise and meet at one point. The union of the removal sets:
// the tip's section along the whole length, and over the section outside
// it the larger of the end edge's and the slope edge's depths in x (both
// roundings of 90 degree edges on the end face: {x < r - sqrt(r^2 - (17.8 +
// r - z)^2)} and {x < r - sqrt(r^2 - (r - t)^2)}, t the distance from the top
// slope), integrated on a grid of 0.004 along the slope by 0.0005 across.
void test_tip_corner_of_three() {
  const TopoDS_Shape body = slab();
  const double v = volume_of(body);
  const std::vector<gp_Pnt2d> s = slab_section();
  const double dl = std::hypot(1.506, 2.6);
  const double dy = 1.506 / dl, dz = 2.6 / dl, ny = dz, nz = -dy;
  const double length = std::hypot(10.9 - kTip, s[6].Y() - kEave) + 0.5;
  for (const double r : {0.9, 1.0}) {
    std::printf(" radius %.1f\n", r);
    const TopoDS_Shape result =
      rounded(body, {slab_edge(body, 0, -1, 0), slab_edge(body, 0, 1, 0.0), slab_edge(body, 6, 0, 0.0)}, r);
    if (result.IsNull()) {
      continue;
    }
    check_closed_solid(result);
    const TipRounding tip = tip_rounding(r);
    // The tip's section: the triangle of the tip, its contact P on the top
    // slope and the obtuse edge O, less the ball.
    const double along = (kInner - kTip) * dy;
    const double across = (kInner - kTip) * ny;
    const double sp = along + std::sqrt(r * r - (r - across) * (r - across));
    const double py = kTip + sp * dy, pz = kEave + sp * dz;
    const auto in_tip = [&](double y, double z) {
      const auto side = [](double ay, double az, double by, double bz, double qy, double qz) {
        return (by - ay) * (qz - az) - (bz - az) * (qy - ay);
      };
      const double s1 = side(kTip, kEave, py, pz, y, z), s2 = side(py, pz, kInner, kEave, y, z),
                   s3 = side(kInner, kEave, kTip, kEave, y, z);
      const bool in = (s1 <= 0 && s2 <= 0 && s3 <= 0) || (s1 >= 0 && s2 >= 0 && s3 >= 0);
      return in && (y - tip.cy) * (y - tip.cy) + (z - tip.cz) * (z - tip.cz) > r * r;
    };
    const double ds = 0.004, dt = 0.0005;
    double rest = 0.0;
    for (double sa = ds / 2; sa < length; sa += ds) {
      for (double t = dt / 2; t < 1.3; t += dt) {
        const double y = kTip + sa * dy + t * ny, z = kEave + sa * dz + t * nz;
        if ((t >= r && z >= kEave + r) || !in_section(s, y, z) || in_tip(y, z)) {
          continue;
        }
        const double h3 = kEave + r - z;
        const double x3 = z < kEave + r ? r - std::sqrt(r * r - h3 * h3) : 0.0;
        const double x20 = t < r ? r - std::sqrt(r * r - (r - t) * (r - t)) : 0.0;
        rest += std::max(x3, x20) * ds * dt;
      }
    }
    check_within(v - volume_of(result), tip.area * kLength + rest, 2e-3, "tip's corner of three: removed");
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_eave_long_edges", test::test_eave_long_edges);
  test::guarded("test_eave_all_edges", test::test_eave_all_edges);
  test::guarded("test_end_face_strip_and_eave_end", test::test_end_face_strip_and_eave_end);
  test::guarded("test_tip_corner_of_three", test::test_tip_corner_of_three);
  if (test::failures == 0) {
    std::puts("test_fillet_eave: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
