// SPDX-License-Identifier: MIT
// A concave rounding along a wall's foot on a rod, where the wall's plan has
// a concave arc tighter than the ball (mitcad#1371), with OCCT's fillet
// alone. The rod is a cylinder of radius 10 about x; the wall standing on it
// is a prism whose front runs through arcs of 0.4 (convex), 1.25 (concave),
// 12 (convex), 1.25 and 0.4, tangent in turn. The ball of radius R cannot
// touch the concave arcs of 1.25: its centre would run round them at a
// radius of 1.25 - R < 0, and OCCT's walk failed there.
//
// The construction (the port's patches 1380 on, third_party/vcpkg-ports):
// - the roundings along the arc of 12 (A) and along the arcs of 0.4 (B) run
//   past the ball that touches the rod and both arcs at once, each until
//   its contact on its wall reaches the concave arc;
// - the concave arc is itself a rolling ball of its own radius rk between
//   the two walls; that ball rolls on down between the wall and the
//   rounding whose end is the higher (here B) and then between the two
//   roundings, until their contacts with the rod meet at P, where the two
//   roundings touch and the ball's section vanishes;
// - the concave arc is cut at the section where that ball meets the higher
//   rounding's end, the roundings are cut along that ball's contacts, and
//   the ball's faces close the corner: a strip (on the wall of 12 and B)
//   and a triangle (on A and B) with its apex at P (a degenerated edge).
//
// At R = 3 the numbers are those of the reported design's stored body
// after its rounding (whose local geometry this body repeats): the corner's
// points, the strip's and the triangle's areas and the volume added. At two
// other radii (2 and 4, still above 1.25) the points that follow from the
// construction's definition alone: P, and the ends of the concave arc's cut.
//
// With MITCAD_GEOMETRY_CASES set, the case `three-face-corner`: `body.brep`,
// the reported design's stored body before the rounding, rounded with 3
// along the same chain (found by its faces), and `after.brep`, the stored
// body after it: about each corner the result lies on the stored body and
// the stored body on the result within 0.01 mm.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

using EdgeFaces = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The wall's plan (a mirror image about x = 0): the arc of 12 about (0, 10),
// the arcs of 0.4 about (+-6.6, kY04) ending at x = +-7, the concave arcs of
// 1.25 tangent to both. The rod: radius 10 about the x axis at y = 0,
// z = 33.
constexpr double kY04     = -1.5868329805053456;
constexpr double kR12     = 12.0;
constexpr double kR04     = 0.4;
constexpr double kRk      = 1.25;
constexpr double kRod     = 10.0;
constexpr double kRodZ    = 33.0;
constexpr double kHalf    = 7.0;
constexpr double kTop     = 51.0;
constexpr double kFoot    = 34.0;
constexpr double kBack    = 3.0;

struct Plan {
  double c125x = 0., c125y = 0.; // the concave arc's centre (x > 0)
  double t1x = 0., t1y = 0.;     // its tangent point with the arc of 12
  double t2x = 0., t2y = 0.;     // its tangent point with the arc of 0.4
};

// The concave arc tangent to the arc of 12 (13.25 from its centre) and to
// the arc of 0.4 (1.65 from its centre), on the ball's side (y < -2).
Plan plan() {
  Plan         p;
  const double ax = 0., ay = 10., ra = kR12 + kRk;
  const double bx = 6.6, by = kY04, rb = kR04 + kRk;
  const double dx = bx - ax, dy = by - ay, d = std::sqrt(dx * dx + dy * dy);
  const double a = (ra * ra - rb * rb + d * d) / (2. * d);
  const double h = std::sqrt(std::max(0., ra * ra - a * a));
  const double mx = ax + a * dx / d, my = ay + a * dy / d;
  const double x1 = mx + h * dy / d, y1 = my - h * dx / d;
  const double x2 = mx - h * dy / d, y2 = my + h * dx / d;
  p.c125x = y1 < y2 ? x1 : x2;
  p.c125y = y1 < y2 ? y1 : y2;
  p.t1x   = ax + kR12 * (p.c125x - ax) / ra;
  p.t1y   = ay + kR12 * (p.c125y - ay) / ra;
  p.t2x   = bx + kR04 * (p.c125x - bx) / rb;
  p.t2y   = by + kR04 * (p.c125y - by) / rb;
  return p;
}

TopoDS_Edge arc(const gp_Pnt& a, const gp_Pnt& m, const gp_Pnt& b) {
  return BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(a, m, b).Value()).Edge();
}

TopoDS_Edge segment(const gp_Pnt& a, const gp_Pnt& b) {
  return BRepBuilderAPI_MakeEdge(GC_MakeSegment(a, b).Value()).Edge();
}

// The point of a circle (centre cx, cy, radius r) halfway between two of its
// points, on the shorter arc.
gp_Pnt mid_on(double cx, double cy, double r, const gp_Pnt& a, const gp_Pnt& b, double z) {
  const double ux = (a.X() - cx) + (b.X() - cx), uy = (a.Y() - cy) + (b.Y() - cy);
  const double n = std::sqrt(ux * ux + uy * uy);
  return gp_Pnt(cx + r * ux / n, cy + r * uy / n, z);
}

// The rod and the wall standing on it, fused, coplanar faces merged.
TopoDS_Shape body() {
  const Plan   p = plan();
  const double z = kFoot;
  // From x = -7 along the front to x = +7, then round the back.
  const gp_Pnt l7(-kHalf, kY04, z), l2(-p.t2x, p.t2y, z), l1(-p.t1x, p.t1y, z);
  const gp_Pnt r1(p.t1x, p.t1y, z), r2(p.t2x, p.t2y, z), r7(kHalf, kY04, z);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(arc(l7, mid_on(-6.6, kY04, kR04, l7, l2, z), l2));
  wire.Add(arc(l2, mid_on(-p.c125x, p.c125y, kRk, l2, l1, z), l1));
  wire.Add(arc(l1, gp_Pnt(0., 10. - kR12, z), r1));
  wire.Add(arc(r1, mid_on(p.c125x, p.c125y, kRk, r1, r2, z), r2));
  wire.Add(arc(r2, mid_on(6.6, kY04, kR04, r2, r7, z), r7));
  const gp_Pnt b7(kHalf, kBack, z), bl(-kHalf, kBack, z);
  wire.Add(segment(r7, b7));
  wire.Add(segment(b7, bl));
  wire.Add(segment(bl, l7));
  const TopoDS_Face  profile = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape wall    = BRepPrimAPI_MakePrism(profile, gp_Vec(0., 0., kTop - kFoot)).Shape();
  // The rod's seam at its bottom, away from the rounding.
  const TopoDS_Shape rod =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-kHalf, 0., kRodZ), gp_Dir(1., 0., 0.), gp_Dir(0., 0., -1.)),
                             kRod,
                             2. * kHalf)
      .Shape();
  BRepAlgoAPI_Fuse fuse(rod, wall);
  if (!fuse.IsDone()) {
    return TopoDS_Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(fuse.Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props, 1e-9);
  return props.Mass();
}

// The edges between the rod (a cylinder of radius 10 about x) and the
// wall's front (cylinders about z): the chain to round.
std::vector<TopoDS_Edge> chain_edges(const TopoDS_Shape& shape) {
  EdgeFaces map;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, map);
  std::vector<TopoDS_Edge> out;
  for (int i = 1; i <= map.Extent(); ++i) {
    if (map(i).Extent() != 2) {
      continue;
    }
    bool rod = false, wall = false;
    for (NCollection_List<TopoDS_Shape>::Iterator it(map(i)); it.More(); it.Next()) {
      const BRepAdaptor_Surface s(TopoDS::Face(it.Value()), false);
      if (s.GetType() != GeomAbs_Cylinder) {
        continue;
      }
      const gp_Cylinder c = s.Cylinder();
      const gp_Dir      a = c.Axis().Direction();
      if (std::abs(std::abs(a.X()) - 1.) < 1e-9 && std::abs(c.Radius() - kRod) < 1e-6) {
        rod = true;
      } else if (std::abs(std::abs(a.Z()) - 1.) < 1e-9) {
        // (The front's arcs only: the reported body's wall is a fin whose
        // back stands on the rod too.)
        for (const double r : {kR04, kRk, kR12}) {
          wall = wall || std::abs(c.Radius() - r) < 1e-6;
        }
      }
    }
    if (rod && wall) {
      out.push_back(TopoDS::Edge(map.FindKey(i)));
    }
  }
  return out;
}

// The fillet of the edges with one radius, as MitCad's dressup calls it.
TopoDS_Shape round_edges(const TopoDS_Shape& shape, const std::vector<TopoDS_Edge>& edges, double radius) {
  BRepFilletAPI_MakeFillet                               fillet(shape);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Edge& e : edges) {
    selected.Add(e);
  }
  fillet.StopChainsAtTurns(selected);
  for (const TopoDS_Edge& e : edges) {
    if (fillet.Contour(e) == 0) {
      fillet.Add(radius, e);
    }
  }
  fillet.CarryChainsAcrossNotches();
  try {
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("the rounding threw: %s\n", failure.what());
    return TopoDS_Shape();
  }
  if (!fillet.IsDone()) {
    std::printf("the rounding failed: %d faulty contours\n", fillet.NbFaultyContours());
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// Whether every edge bounds two faces (a seam its face twice): a closed
// shell. Internal edges of a face (the reported body has some) bound none.
bool closed(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> uses;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      const TopAbs_Orientation o = e.Current().Orientation();
      if (BRep_Tool::Degenerated(TopoDS::Edge(e.Current())) || o == TopAbs_INTERNAL || o == TopAbs_EXTERNAL) {
        continue;
      }
      const int i = uses.FindIndex(e.Current());
      if (i == 0) {
        uses.Add(e.Current(), 1);
      } else {
        ++uses(i);
      }
    }
  }
  for (int i = 1; i <= uses.Extent(); ++i) {
    if (uses(i) != 2) {
      return false;
    }
  }
  return true;
}

// The distance from a point to the nearest vertex of the shape.
double nearest_vertex(const TopoDS_Shape& shape, const gp_Pnt& p) {
  double best = 1e100;
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    best = std::min(best, BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p));
  }
  return best;
}

// Whether a face has vertices near each of the points.
bool has_vertices(const TopoDS_Shape& face, const std::vector<gp_Pnt>& points, double tol) {
  for (const gp_Pnt& p : points) {
    if (nearest_vertex(face, p) > tol) {
      return false;
    }
  }
  return true;
}

// The faces with vertices near each of the points.
std::vector<TopoDS_Face> faces_with(const TopoDS_Shape& shape, const std::vector<gp_Pnt>& points, double tol) {
  std::vector<TopoDS_Face> out;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    if (has_vertices(f.Current(), points, tol)) {
      out.push_back(TopoDS::Face(f.Current()));
    }
  }
  return out;
}

// Whether the face has a degenerated edge at the point.
bool degenerated_at(const TopoDS_Face& face, const gp_Pnt& p, double tol) {
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge) && nearest_vertex(edge, p) <= tol) {
      return true;
    }
  }
  return false;
}

// The points the construction's definition gives for a radius R (x > 0):
// P, where the ball touching the rod and both convex arcs touches the rod;
// the cut of the concave arc at the height of the ball at B's end, on its
// edges with the arc of 0.4 (W) and with the arc of 12 (K).
struct Corner {
  gp_Pnt p, w, k;
};

Corner corner_points(double radius) {
  const Plan p = plan();
  Corner     c;
  // The ball's centre: 12 + R from (0, 10), 0.4 + R from (6.6, kY04), on the
  // ball's side.
  {
    const double ax = 0., ay = 10., ra = kR12 + radius;
    const double bx = 6.6, by = kY04, rb = kR04 + radius;
    const double dx = bx - ax, dy = by - ay, d = std::sqrt(dx * dx + dy * dy);
    const double a = (ra * ra - rb * rb + d * d) / (2. * d);
    const double h = std::sqrt(std::max(0., ra * ra - a * a));
    const double mx = ax + a * dx / d, my = ay + a * dy / d;
    const double x1 = mx + h * dy / d, y1 = my - h * dx / d;
    const double x2 = mx - h * dy / d, y2 = my + h * dx / d;
    const double cx = y1 < y2 ? x1 : x2, cy = y1 < y2 ? y1 : y2;
    const double cz = kRodZ + std::sqrt((kRod + radius) * (kRod + radius) - cy * cy);
    const double k  = kRod / (kRod + radius);
    c.p             = gp_Pnt(cx, k * cy, kRodZ + k * (cz - kRodZ));
  }
  // B's end: its contact at the tangent point of the arcs of 0.4 and 1.25.
  {
    const double by = kY04, rb = kR04 + kRk;
    const double uy =(p.c125y - by) / rb;
    const double cy = by + (kR04 + radius) * uy;
    const double z  =kRodZ + std::sqrt((kRod + radius) * (kRod + radius) - cy * cy);
    c.w            = gp_Pnt(p.t2x, p.t2y, z);
    c.k            = gp_Pnt(p.t1x, p.t1y, z);
  }
  return c;
}

gp_Pnt mirrored(const gp_Pnt& p) {
  return gp_Pnt(-p.X(), p.Y(), p.Z());
}

// The rounding at one radius: valid, closed, P and the cut of each concave
// arc where the construction puts them, a triangle with its apex at P.
TopoDS_Shape check_radius(const TopoDS_Shape& shape, double radius) {
  const std::vector<TopoDS_Edge> edges = chain_edges(shape);
  CHECK(edges.size() == 5);
  const TopoDS_Shape result = round_edges(shape, edges, radius);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return result;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  const Corner c = corner_points(radius);
  for (const bool mirror : {false, true}) {
    const gp_Pnt p = mirror ? mirrored(c.p) : c.p;
    const gp_Pnt w = mirror ? mirrored(c.w) : c.w;
    const gp_Pnt k = mirror ? mirrored(c.k) : c.k;
    const double dp = nearest_vertex(result, p), dw = nearest_vertex(result, w), dk = nearest_vertex(result, k);
    std::printf("radius %.2f%s: P (%.5f %.5f %.5f) off %.2e, the arc's cut at z %.5f off %.2e and %.2e\n",
                radius,
                mirror ? " (x < 0)" : "",
                p.X(),
                p.Y(),
                p.Z(),
                dp,
                w.Z(),
                dw,
                dk);
    CHECK(dp <= 1e-4);
    CHECK(dw <= 1e-4);
    CHECK(dk <= 1e-4);
    bool apex = false;
    for (const TopoDS_Face& f : faces_with(result, {p}, 1e-4)) {
      apex = apex || degenerated_at(f, p, 1e-4);
    }
    CHECK(apex);
  }
  return result;
}

void test_radius_of_the_reported_design() {
  const TopoDS_Shape shape = body();
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  const TopoDS_Shape result = check_radius(shape, 3.);
  if (result.IsNull()) {
    return;
  }
  // The stored body's corner (x > 0; the other one its mirror image): P,
  // the triangle's other corners Q (on the wall of 12) and Q2 (on B), the
  // concave arc's cut W (on B) and K (on the wall of 12).
  const gp_Pnt P(4.54439, -3.30389, 42.43845), Q(4.58956, -1.08765, 45.41385);
  const gp_Pnt Q2(6.17612, -1.68896, 45.24883), W(6.23228, -1.74425, 45.66669), K(4.60360, -1.08182, 45.66669);
  for (const bool mirror : {false, true}) {
    const auto at = [mirror](const gp_Pnt& x) { return mirror ? mirrored(x) : x; };
    for (const gp_Pnt& x : {Q, Q2}) {
      const double d = nearest_vertex(result, at(x));
      std::printf("corner point (%.5f %.5f %.5f) off %.2e\n", at(x).X(), at(x).Y(), at(x).Z(), d);
      CHECK(d <= 1e-3);
    }
    const std::vector<TopoDS_Face> triangle = faces_with(result, {at(P), at(Q), at(Q2)}, 1e-3);
    const std::vector<TopoDS_Face> strip    = faces_with(result, {at(Q), at(Q2), at(W), at(K)}, 1e-3);
    CHECK(triangle.size() == 1);
    CHECK(strip.size() == 1);
    if (triangle.size() == 1 && strip.size() == 1) {
      const double ta = area_of(triangle.front()), as = area_of(strip.front());
      std::printf("the triangle's area %.6f (the stored body's 3.741647), the strip's %.6f (0.683919)\n", ta, as);
      CHECK_NEAR_TOL(ta, 3.741647, 0.01);
      CHECK_NEAR_TOL(as, 0.683919, 0.01);
    }
  }
  const double added = volume_of(result) - volume_of(shape);
  std::printf("the material added %.6f mm3 (the stored body's 15.038261)\n", added);
  CHECK(std::abs(added - 15.038261) <= 0.01);
}

void test_other_radii() {
  const TopoDS_Shape shape = body();
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  for (const double radius : {2., 4.}) {
    const TopoDS_Shape result = check_radius(shape, radius);
    if (!result.IsNull()) {
      std::printf("radius %.2f: the material added %.6f mm3\n", radius, volume_of(result) - volume_of(shape));
      CHECK(volume_of(result) > volume_of(shape));
    }
  }
}

std::string geometry_cases() {
#ifdef _MSC_VER
  char*       value  = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, "MITCAD_GEOMETRY_CASES") == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv("MITCAD_GEOMETRY_CASES");
  return value != nullptr ? std::string(value) : std::string();
#endif
}

TopoDS_Shape read(const std::string& path) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  BRepTools::Read(shape, path.c_str(), builder);
  return shape;
}

// The faces of a shape whose box meets the region.
TopoDS_Compound faces_in(const TopoDS_Shape& shape, const Bnd_Box& region) {
  TopoDS_Compound out;
  BRep_Builder    builder;
  builder.MakeCompound(out);
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    if (!box.IsOut(region)) {
      builder.Add(out, f.Current());
    }
  }
  return out;
}

// The farthest of points inside the faces of `from` within the region from
// the faces of `to` about it.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to, const Bnd_Box& region) {
  Bnd_Box wider = region;
  wider.Enlarge(1.);
  const TopoDS_Compound target = faces_in(to, wider);
  double                worst  = 0.;
  for (TopExp_Explorer f(faces_in(from, region), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 10; ++i) {
      for (int j = 0; j <= 10; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 10.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 10.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = s.Value(u, v);
        if (region.IsOut(p)) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), target);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/three-face-corner/";
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  // (Six edges: the stored body's arc of 12 meets the rod in two.)
  const std::vector<TopoDS_Edge> edges = chain_edges(shape);
  CHECK(edges.size() == 6);
  const TopoDS_Shape result = round_edges(shape, edges, 3.);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  for (const double side : {1., -1.}) {
    Bnd_Box region;
    region.Update(side > 0. ? 3. : -7.1, -4.5, 41.8, side > 0. ? 7.1 : -3., 0., 46.2);
    const double out  = farthest(result, after, region);
    const double back = farthest(after, result, region);
    std::printf("about the corner at x %s 0: the result on the stored body within %.6f mm, the stored body on it "
                "within %.6f mm\n",
                side > 0. ? ">" : "<",
                out,
                back);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
  }
  // Along the whole chain (the rounding's region).
  Bnd_Box region;
  region.Update(-7.1, -5., 41.5, 7.1, 0.5, 46.5);
  const double out  = farthest(result, after, region);
  const double back = farthest(after, result, region);
  std::printf("along the chain: %.6f mm and %.6f mm\n", out, back);
  CHECK(out <= 0.01);
  CHECK(back <= 0.01);
  const double added = volume_of(result) - volume_of(shape);
  const double file  = volume_of(after) - volume_of(shape);
  std::printf("the material added %.6f mm3, the stored body's %.6f\n", added, file);
  CHECK(std::abs(added - file) <= 0.02 * std::abs(file));
}

// The case `fillet-trimmed-torus` of MITCAD_GEOMETRY_CASES: the reported
// design's body as MitCad replays it before a later rounding of the same
// design (a key's top edge, 13 long, rounded with 2 over its narrow side up
// to the key's end roundings and a bore's wall, a torus; port patch 0940),
// which mitcad#1371's corner now lets the replay reach. The torus face lies
// on a trimmed surface of the torus; with the port's patch 1381 the
// rounding builds, valid and closed (before, its ends failed: 0940 found
// no run-on of the torus's junction with an end rounding on the trimmed
// surface, and then put its curve on the torus a period off).
void test_rounding_over_a_trimmed_torus() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_rounding_over_a_trimmed_torus: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const TopoDS_Shape shape = read(cases + "/fillet-trimmed-torus/body.brep");
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  // The key's top edge: the line between (-6.5, 20.7732, 164.0066) and
  // (6.5, 20.7732, 164.0066).
  const gp_Pnt             a(-6.5, 20.7732, 164.0066), b(6.5, 20.7732, 164.0066);
  std::vector<TopoDS_Edge> edges;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    TopoDS_Vertex      v1, v2;
    TopExp::Vertices(edge, v1, v2);
    if (v1.IsNull() || v2.IsNull()) {
      continue;
    }
    const gp_Pnt p1 = BRep_Tool::Pnt(v1), p2 = BRep_Tool::Pnt(v2);
    if ((p1.Distance(a) < 1e-3 && p2.Distance(b) < 1e-3) || (p1.Distance(b) < 1e-3 && p2.Distance(a) < 1e-3)) {
      bool seen = false;
      for (const TopoDS_Edge& known : edges) {
        seen = seen || known.IsSame(edge);
      }
      if (!seen) {
        edges.push_back(edge);
      }
    }
  }
  CHECK(edges.size() == 1);
  if (edges.size() != 1) {
    return;
  }
  const TopoDS_Shape result = round_edges(shape, edges, 2.);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  std::printf("the key's top rounded over the trimmed torus: the material taken %.6f mm3\n",
              volume_of(shape) - volume_of(result));
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_radius_of_the_reported_design", test::test_radius_of_the_reported_design);
  test::guarded("test_other_radii", test::test_other_radii);
  test::guarded("test_reported_body", test::test_reported_body);
  test::guarded("test_rounding_over_a_trimmed_torus", test::test_rounding_over_a_trimmed_torus);
  if (test::failures == 0) {
    std::puts("test_fillet_three_face_corner: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
