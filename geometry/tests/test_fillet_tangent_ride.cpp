// SPDX-License-Identifier: MIT
// Two roundings of one fillet whose blends overlap on a face narrower than
// both, the later one's ball rolling tangent on the earlier one's blend
// (mitcad#1926, the port's patches 1720 and 1721), with OCCT's fillet alone.
//
// Synthetic prisms 20 long along x, their sections in the (y, z) plane:
// - a wall w wide and 10 high, its top's edges rounded with 2 (at y = 0) and
//   with 1 (at y = w), w between 2 and 3 (both convex);
// - an L: a plate 5 thick and a tower 10 high standing w from the plate's
//   back edge, the tower's foot rounded with 2 (concave), the plate's back
//   edge with 1 (convex);
// - a step h high on a plate, its foot rounded with 1 (concave), its top's
//   edge with 0.5 (convex), h below 1.5.
// The face between the two edges goes; the smaller ball touches its other
// face and the larger rounding's blend, which its contact there cuts (the
// shape the two roundings make one after the other, as the bodies the
// `.f3d` files store have it). The result is valid and its volume that of
// the section made of those arcs (within a millionth): before the patch the
// wall's rounding of 1 rolled on the contact line of the rounding of 2 on
// the top (more material) and the other two failed. Both orders in which
// the edges are added.
//
// Part rides (1721): the face between narrower than the two roundings from
// a crossing of their contacts to an end only (a wall's top narrowing from
// 3.5 to 2.5, either edge slanting; a step whose height falls from 2 to
// 1.3). The smaller ball rides on the larger blend from that end to the
// crossing and runs on the face beyond it. The result is valid and its
// volume that of the two roundings made one after the other, the larger
// first (within a millionth); before 1721 the two blends ran through each
// other past the crossing (the wall's result with 0.45 mm3 more material).
//
// Ride ends at a corner with one more rounding (1722): the wall 2.5 wide
// with an edge at an end rounded too (0.5 or 1, at either side, at one or
// both ends, several at once). The end vertex of the ride's stripe holds
// that rounding as well: it is cut by the ride's blend there, the ride's
// blend by it and by the end face. The result is valid and its volume that
// of the two top roundings less what each end rounding takes away below
// the top's arcs (within a millionth), the wall a prism or a box; before
// 1722 no ride was made there and the top roundings rolled on the larger
// one's contact line (more material).
//
// Corners of three blends (1723): the end edge's rounding reaches past the
// ride's point (1.2 or 1.5 beside the rounding of 2, 1.7 beside the
// rounding of 1), its contact on the end face meeting the ride's other
// blend; the three blends meet on the ride's line. The same checks.
//
// Rides between two crossings (1724): a plate's back edge running past a
// tower of half a cylinder (its round side towards the edge, 2.3 or 2.5 from
// it at the apex), the tower's foot rounded with 2 (concave) and the back
// edge with 1: the plate is narrower than the two roundings around the
// apex only. The result is valid and its volume that of the two roundings
// made one after the other, the larger first (within a millionth), both
// orders; before 1724 the fillet failed. The same with the plate's corners
// rounded in plan, so that the back edge's rounding runs round the whole
// rim, a closed contour (1725), the tower at the back edge's middle or off
// it; before 1725 the fillet failed (the blends intersecting).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <Geom_Curve.hxx>
#include <GProp_GProps.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

namespace {

int failures = 0;

void check(bool condition, const std::string& what) {
  if (!condition) {
    std::fprintf(stderr, "check failed: %s\n", what.c_str());
    ++failures;
  }
}

constexpr double kPi    = 3.14159265358979323846;
constexpr double kLength = 20.0;

struct P2 {
  double y;
  double z;
};

// The prism of a polygonal section in the plane x = 0, kLength along x.
TopoDS_Shape prism(const std::vector<P2>& section) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const P2& p : section) {
    polygon.Add(gp_Pnt(0, p.y, p.z));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire()).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(kLength, 0, 0)).Shape();
}

// The prism's edge along x through (y, z).
TopoDS_Edge edge_at(const TopoDS_Shape& body, double y, double z) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt            m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(m.X() - 0.5 * kLength) < 1e-7 && std::abs(m.Y() - y) < 1e-7 && std::abs(m.Z() - z) < 1e-7) {
      return TopoDS::Edge(ex.Current());
    }
  }
  return TopoDS_Edge();
}

// The area of a closed polygon.
double area(const std::vector<P2>& p) {
  double a = 0;
  for (std::size_t i = 0; i < p.size(); ++i) {
    const P2& q = p[i];
    const P2& r = p[(i + 1) % p.size()];
    a += q.y * r.z - r.y * q.z;
  }
  return 0.5 * std::abs(a);
}

// An arc about c of radius r from angle a0 to a1, as a polyline.
void arc(std::vector<P2>& p, P2 c, double r, double a0, double a1) {
  const int n = 4000;
  for (int i = 0; i <= n; ++i) {
    const double a = a0 + (a1 - a0) * i / n;
    p.push_back({c.y + r * std::cos(a), c.z + r * std::sin(a)});
  }
}

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// Rounds the edges with their radii and checks the result: done, valid,
// its volume the expected one within a millionth.
void round_and_check(const std::string& name, const TopoDS_Shape& body,
                     const std::vector<std::pair<TopoDS_Edge, double>>& edges, double expected) {
  BRepFilletAPI_MakeFillet maker(body);
  for (const auto& e : edges) {
    check(!e.first.IsNull(), name + ": the edge is found");
    if (e.first.IsNull()) {
      return;
    }
    maker.Add(e.second, e.first);
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "%s: the fillet throws: %s\n", name.c_str(), e.what());
  }
  check(maker.IsDone(), name + ": the fillet builds");
  if (!maker.IsDone()) {
    return;
  }
  check(BRepCheck_Analyzer(maker.Shape()).IsValid(), name + ": the result is valid");
  const double v = volume(maker.Shape());
  if (std::abs(v - expected) > 1e-6 * expected) {
    std::fprintf(stderr, "%s: volume %.6f, expected %.6f\n", name.c_str(), v, expected);
  }
  check(std::abs(v - expected) <= 1e-6 * expected, name + ": the volume is the section's");
}

// The wall w wide, its top's edges rounded with 2 and 1.
void test_wall(double w) {
  const double h = 10, r2 = 2, r1 = 1;
  const TopoDS_Shape body = prism({{0, 0}, {w, 0}, {w, h}, {0, h}});
  // The ball of 1 inside the rounding of 2's circle, touching it and the
  // side y = w.
  const P2     c2{r2, h - r2};
  const double dy = (w - r1) - c2.y;
  const double dz = std::sqrt((r2 - r1) * (r2 - r1) - dy * dy);
  const P2     c1{w - r1, c2.z + dz};
  const double at = std::atan2(dz, dy);
  std::vector<P2> section{{0, 0}, {w, 0}};
  arc(section, c1, r1, 0, at);
  arc(section, c2, r2, at, kPi);
  const double expected = area(section) * kLength;
  const TopoDS_Edge inner = edge_at(body, 0, h), outer = edge_at(body, w, h);
  const std::string name  = "wall " + std::to_string(w);
  round_and_check(name, body, {{inner, r2}, {outer, r1}}, expected);
  round_and_check(name + " (the other order)", body, {{outer, r1}, {inner, r2}}, expected);
}

// The L: a tower standing w from the plate's back edge, its foot rounded
// with 2, the plate's back edge with 1.
void test_tower_foot(double w) {
  const double t = 5, plate = 30, tower = 10, height = 10, r2 = 2, r1 = 1;
  const TopoDS_Shape body = prism(
    {{0, -t}, {plate, -t}, {plate, 0}, {w + tower, 0}, {w + tower, height}, {w, height}, {w, 0}, {0, 0}});
  // The ball of 1 inside the plate, touching its back face and, from
  // outside, the concave rounding's circle.
  const P2     c2{w - r2, r2};
  const double dy = r1 - c2.y;
  const double dz = -std::sqrt((r2 + r1) * (r2 + r1) - dy * dy);
  const P2     c1{r1, c2.z + dz};
  const double at = std::atan2(dz, dy);
  std::vector<P2> section{{0, -t}, {plate, -t}, {plate, 0}, {w + tower, 0}, {w + tower, height}, {w, height}};
  arc(section, c2, r2, 0, at);
  arc(section, c1, r1, at + kPi, kPi);
  const double      expected = area(section) * kLength;
  const TopoDS_Edge foot = edge_at(body, w, 0), back = edge_at(body, 0, 0);
  const std::string name = "tower foot " + std::to_string(w);
  round_and_check(name, body, {{foot, r2}, {back, r1}}, expected);
  round_and_check(name + " (the other order)", body, {{back, r1}, {foot, r2}}, expected);
}

// The step h high, its foot rounded with 1, its top's edge with 0.5.
void test_step(double h) {
  const double       y = 10, r2 = 1, r1 = 0.5;
  const TopoDS_Shape body = prism({{0, -5}, {30, -5}, {30, 0}, {y, 0}, {y, h}, {0, h}});
  // The ball of 0.5 inside the step, touching its top and, from outside,
  // the concave rounding's circle.
  const P2     c2{y + r2, r2};
  const double dz = (h - r1) - c2.z;
  const double dy = -std::sqrt((r2 + r1) * (r2 + r1) - dz * dz);
  const P2     c1{c2.y + dy, h - r1};
  const double at = std::atan2(dz, dy) + 2 * kPi; // from c2 towards c1
  std::vector<P2> section{{0, -5}, {30, -5}, {30, 0}};
  arc(section, c2, r2, 1.5 * kPi, at);
  arc(section, c1, r1, at - kPi, 0.5 * kPi);
  section.push_back({0, h});
  const double      expected = area(section) * kLength;
  const TopoDS_Edge foot = edge_at(body, y, 0), top = edge_at(body, y, h);
  const std::string name = "step " + std::to_string(h);
  round_and_check(name, body, {{foot, r2}, {top, r1}}, expected);
  round_and_check(name + " (the other order)", body, {{top, r1}, {foot, r2}}, expected);
}

// A block 30 x 20 x 7 cut back to a wall 2 thick at x = 0 to 2 above z = 3
// (booleans of boxes; its edges' directions as the booleans leave them),
// the wall's top rounded with 1.5 along x = 0 and 0.8 along x = 2.
void test_wall_of_a_block() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(30, 20, 7)).Shape();
  const TopoDS_Shape cut   = BRepPrimAPI_MakeBox(gp_Pnt(2, -1, 3), gp_Pnt(31, 21, 8)).Shape();
  const TopoDS_Shape body  = BRepAlgoAPI_Cut(block, cut).Shape();
  const double       h = 7, w = 2, r2 = 1.5, r1 = 0.8;
  TopoDS_Edge        outer, inner;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt            m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(m.Y() - 10) < 1e-7 && std::abs(m.Z() - h) < 1e-7) {
      (std::abs(m.X()) < 1e-7 ? outer : inner) = TopoDS::Edge(ex.Current());
    }
  }
  const P2     c2{r2, h - r2};
  const double dy = (w - r1) - c2.y;
  const double dz = std::sqrt((r2 - r1) * (r2 - r1) - dy * dy);
  const P2     c1{w - r1, c2.z + dz};
  const double at = std::atan2(dz, dy);
  std::vector<P2> section{{0, 3}, {w, 3}};
  arc(section, c1, r1, 0, at);
  arc(section, c2, r2, at, kPi);
  const double expected = 30.0 * 20 * 3 + area(section) * 20;
  round_and_check("wall of a block", body, {{outer, r2}, {inner, r1}}, expected);
  round_and_check("wall of a block (the other order)", body, {{inner, r1}, {outer, r2}}, expected);
}

// The straight edge of the body through point p, inside it.
TopoDS_Edge edge_through(const TopoDS_Shape& body, const gp_Pnt& p) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    double                        f = 0, l = 0;
    const Handle(Geom_Curve)      c = BRep_Tool::Curve(TopoDS::Edge(ex.Current()), f, l);
    if (c.IsNull()) {
      continue;
    }
    GeomAPI_ProjectPointOnCurve pr(p, c, f, l);
    if (pr.NbPoints() > 0 && pr.LowerDistance() < 1e-7 && pr.LowerDistanceParameter() > f + 1e-7 &&
        pr.LowerDistanceParameter() < l - 1e-7) {
      return TopoDS::Edge(ex.Current());
    }
  }
  return TopoDS_Edge();
}

// The prism of a polygonal section in the plane z = 0, h high.
TopoDS_Shape prism_up(const std::vector<gp_Pnt>& section, double h) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt& p : section) {
    polygon.Add(p);
  }
  polygon.Close();
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(polygon.Wire()).Face(), gp_Vec(0, 0, h)).Shape();
}

// The volume of the body rounded at the edge through pa with ra, then at the
// edge through pb of that result with rb; negative where either fails.
double one_after_the_other(const TopoDS_Shape& body, const gp_Pnt& pa, double ra, const gp_Pnt& pb, double rb) {
  TopoDS_Shape shape = body;
  for (const auto& e : {std::make_pair(pa, ra), std::make_pair(pb, rb)}) {
    const TopoDS_Edge edge = edge_through(shape, e.first);
    if (edge.IsNull()) {
      return -1;
    }
    BRepFilletAPI_MakeFillet maker(shape);
    maker.Add(e.second, edge);
    try {
      OCC_CATCH_SIGNALS
      maker.Build();
    } catch (const Standard_Failure&) {
      return -1;
    }
    if (!maker.IsDone()) {
      return -1;
    }
    shape = maker.Shape();
  }
  return volume(shape);
}

// A part ride: the edge through pa rounded with ra (the larger), the one
// through pb with rb, together, against the two made one after the other.
void test_part_ride(const std::string& name, const TopoDS_Shape& body, const gp_Pnt& pa, double ra,
                    const gp_Pnt& pb, double rb) {
  const double expected = one_after_the_other(body, pa, ra, pb, rb);
  check(expected > 0, name + ": the two roundings build one after the other");
  if (expected <= 0) {
    return;
  }
  const TopoDS_Edge a = edge_through(body, pa), b = edge_through(body, pb);
  round_and_check(name, body, {{a, ra}, {b, rb}}, expected);
  round_and_check(name + " (the other order)", body, {{b, rb}, {a, ra}}, expected);
}

// A wall 10 high whose top narrows from 3.5 to 2.5 along x, its top's
// edges rounded with 2 and 1, the edge rounded with 1 slanting (slant_b) or
// the one rounded with 2; narrow at x = 0 (or at x = 20, flipped).
void test_narrowing_wall(bool slant_b, bool flipped) {
  const double h = 10;
  const double n = flipped ? 3.5 : 2.5, w = flipped ? 2.5 : 3.5;
  TopoDS_Shape body;
  gp_Pnt       pa, pb;
  if (slant_b) {
    body = prism_up({gp_Pnt(0, 0, 0), gp_Pnt(20, 0, 0), gp_Pnt(20, w, 0), gp_Pnt(0, n, 0)}, h);
    pa   = gp_Pnt(10, 0, h);
    pb   = gp_Pnt(10, 0.5 * (n + w), h);
  } else {
    body = prism_up({gp_Pnt(0, 2.5 - n, 0), gp_Pnt(20, 2.5 - w, 0), gp_Pnt(20, 2.5, 0), gp_Pnt(0, 2.5, 0)}, h);
    pa   = gp_Pnt(10, 2.5 - 0.5 * (n + w), h);
    pb   = gp_Pnt(10, 2.5, h);
  }
  test_part_ride(std::string("narrowing wall, ") + (slant_b ? "its 1 slanting" : "its 2 slanting") +
                     (flipped ? ", narrow at x = 20" : ", narrow at x = 0"),
                 body, pa, 2, pb, 1);
}

// A step on a plate, its top falling from 2 high at x = 20 to 1.3 at x = 0,
// its foot rounded with 1 (concave), its top edge with 0.5.
void test_falling_step() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -5), gp_Pnt(20, 30, 0)).Shape();
  BRepBuilderAPI_MakePolygon polygon;
  polygon.Add(gp_Pnt(0, 0, 0));
  polygon.Add(gp_Pnt(20, 0, 0));
  polygon.Add(gp_Pnt(20, 0, 2.0));
  polygon.Add(gp_Pnt(0, 0, 1.3));
  polygon.Close();
  const TopoDS_Shape step =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(polygon.Wire()).Face(), gp_Vec(0, 10, 0)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, step).Shape());
  unify.Build();
  test_part_ride("falling step", unify.Shape(), gp_Pnt(10, 10, 0), 1, gp_Pnt(10, 10, 1.65), 0.5);
}

// The wall 2.5 wide, its top's edges rounded with 2 (y = 0) and 1 (y = w),
// and vertical edges at its ends rounded with the radii given: (side 0 at
// y = 0 or 1 at y = w, end 0 at x = 0 or 1 at x = kLength, radius).
struct EndEdge {
  int    side;
  int    end;
  double radius;
};

// The body a prism of the section, or a box (its faces and edges oriented
// otherwise).
void test_wall_end_corners(const std::string& name, const std::vector<EndEdge>& ends, bool box) {
  const double       h = 10, w = 2.5, r2 = 2, r1 = 1;
  const TopoDS_Shape body = box ? BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(kLength, w, h)).Shape()
                                : prism({{0, 0}, {w, 0}, {w, h}, {0, h}});
  // The section of the two top roundings, as test_wall makes it.
  const P2     c2{r2, h - r2};
  const double dy = (w - r1) - c2.y;
  const double dz = std::sqrt((r2 - r1) * (r2 - r1) - dy * dy);
  const P2     c1{w - r1, c2.z + dz};
  const double at = std::atan2(dz, dy);
  std::vector<P2> section{{0, 0}, {w, 0}};
  arc(section, c1, r1, 0, at);
  arc(section, c2, r2, at, kPi);
  double expected = area(section) * kLength;
  // The top's height over y: the larger arc up to the ride's point, then the
  // smaller one.
  const double yRide = c2.y + r2 * std::cos(at);
  const auto   top   = [&](double y) {
    return y <= yRide ? c2.z + std::sqrt(std::max(0.0, r2 * r2 - (y - c2.y) * (y - c2.y)))
                          : c1.z + std::sqrt(std::max(0.0, r1 * r1 - (y - c1.y) * (y - c1.y)));
  };
  // What a rounding of radius r of a vertical edge at a side takes away: at
  // the distance t from that side, the stretch from the end face to the
  // ball's circle, under the top (Simpson's rule).
  const auto taken = [&](int side, double r) {
    const int n   = 20000;
    double    sum = 0;
    for (int i = 0; i <= n; ++i) {
      const double t = r * i / n;
      const double y = side == 0 ? t : w - t;
      const double x = r - std::sqrt(std::max(0.0, r * r - (t - r) * (t - r)));
      const double f = x * top(y);
      sum += f * (i == 0 || i == n ? 1 : (i % 2 == 1 ? 4 : 2));
    }
    return sum * (r / n) / 3;
  };
  std::vector<std::pair<TopoDS_Edge, double>> edges{{edge_at(body, 0, h), r2}, {edge_at(body, w, h), r1}};
  for (const EndEdge& e : ends) {
    expected -= taken(e.side, e.radius);
    edges.push_back({edge_through(body, gp_Pnt(e.end == 0 ? 0 : kLength, e.side == 0 ? 0 : w, 0.5 * h)), e.radius});
  }
  round_and_check(name, body, edges, expected);
  std::reverse(edges.begin(), edges.end());
  round_and_check(name + " (the other order)", body, edges, expected);
}

// The plate 30 x 20 x 5 below z = 0 and the tower: half a cylinder of
// radius 6 and height 10 about (15, 20 - 6 - gap), its round side towards
// the back edge (y = 20).
void test_tower_behind_the_back_edge(double gap) {
  const double       cy = 20 - 6 - gap;
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -5), gp_Pnt(30, 20, 0)).Shape();
  const TopoDS_Shape cylinder =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(15, cy, 0), gp_Dir(0, 0, 1)), 6, 10).Shape();
  const TopoDS_Shape half = BRepAlgoAPI_Cut(cylinder, BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(30, cy, 10)).Shape()).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, half).Shape());
  unify.Build();
  test_part_ride("tower " + std::to_string(gap) + " from the back edge", unify.Shape(), gp_Pnt(15, cy + 6, 0), 2,
                 gp_Pnt(15, 20, 0), 1);
}

// As above, the plate's corners in plan rounded with 4, so that the back
// edge's rounding runs on round the whole rim (a closed contour of eight
// edges), the tower x0 along the back edge.
void test_tower_behind_a_rounded_rim(double gap, double x0) {
  const double cy = 20 - 6 - gap;
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -5), gp_Pnt(30, 20, 0)).Shape();
  {
    BRepFilletAPI_MakeFillet corners(plate);
    for (TopExp_Explorer ex(plate, TopAbs_EDGE); ex.More(); ex.Next()) {
      const BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
      const gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
      if (std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9) {
        corners.Add(4, TopoDS::Edge(ex.Current()));
      }
    }
    corners.Build();
    check(corners.IsDone(), "the plate's corners are rounded");
    if (!corners.IsDone()) {
      return;
    }
    plate = corners.Shape();
  }
  const TopoDS_Shape cylinder =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x0, cy, 0), gp_Dir(0, 0, 1)), 6, 10).Shape();
  const TopoDS_Shape half = BRepAlgoAPI_Cut(cylinder, BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(30, cy, 10)).Shape()).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, half).Shape());
  unify.Build();
  test_part_ride("tower " + std::to_string(gap) + " from a rounded rim's back edge at " + std::to_string(x0),
                 unify.Shape(), gp_Pnt(x0, cy + 6, 0), 2, gp_Pnt(x0, 20, 0), 1);
}

} // namespace

int main() {
  try {
    for (const double w : {2.9, 2.5, 2.127}) {
      test_wall(w);
    }
    for (const double w : {2.9, 2.5, 2.2}) {
      test_tower_foot(w);
    }
    for (const double h : {1.45, 1.3}) {
      test_step(h);
    }
    test_wall_of_a_block();
    for (const bool slant_b : {true, false}) {
      for (const bool flipped : {false, true}) {
        test_narrowing_wall(slant_b, flipped);
      }
    }
    test_falling_step();
    for (const double gap : {2.3, 2.5}) {
      test_tower_behind_the_back_edge(gap);
    }
    for (const double gap : {2.3, 2.5}) {
      for (const double x0 : {15.0, 12.0}) {
        test_tower_behind_a_rounded_rim(gap, x0);
      }
    }
    for (const bool box : {false, true}) {
      const std::string of = box ? " (a box)" : "";
      test_wall_end_corners("wall, an end's edge at the 2 rounded with 0.5" + of, {{0, 0, 0.5}}, box);
      test_wall_end_corners("wall, an end's edge at the 2 rounded with 1" + of, {{0, 0, 1}}, box);
      test_wall_end_corners("wall, an end's edge at the 1 rounded with 0.5" + of, {{1, 0, 0.5}}, box);
      test_wall_end_corners("wall, the other end's edges" + of, {{0, 1, 0.5}, {1, 1, 0.5}}, box);
      test_wall_end_corners("wall, an end's two edges" + of, {{0, 0, 1}, {1, 0, 0.5}}, box);
      test_wall_end_corners("wall, both ends' edges at the 2" + of, {{0, 0, 0.5}, {0, 1, 0.5}}, box);
      test_wall_end_corners("wall, an end's edge at the 2 rounded with 1.2" + of, {{0, 0, 1.2}}, box);
      test_wall_end_corners("wall, an end's edge at the 2 rounded with 1.5" + of, {{0, 0, 1.5}}, box);
      test_wall_end_corners("wall, an end's edge at the 1 rounded with 1.7" + of, {{1, 0, 1.7}}, box);
      test_wall_end_corners("wall, the other end's edge at the 2 rounded with 1.2" + of, {{0, 1, 1.2}}, box);
    }
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "unexpected OCCT error: %s\n", e.what());
    ++failures;
  }
  if (failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("test_fillet_tangent_ride: all checks passed\n");
  return 0;
}
