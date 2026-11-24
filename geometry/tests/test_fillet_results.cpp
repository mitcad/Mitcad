// SPDX-License-Identifier: MIT
// Roundings whose result OCCT's topological build made invalid, or whose
// computation failed, rebuilt with the port's patches 0150 and later
// (third_party/vcpkg-ports/opencascade, mitcad#239; a rounding carried
// across a notch in its rim, 0260, mitcad#307; a rounding round wall corners
// of the ball's radius under a steep top, 0670, and a rounding ending on a
// bore along the line of its seam, 0690, and a rim rounded up to a rib whose
// roundings continue the bottom, 0691 and 0692, mitcad#235; a rounding ending
// on a bore split round the corner into faces of one surface, 0740, and the
// rim's contact ending where the rib's edge stops short of its vertex, 0741,
// mitcad#415). OCCT alone on plain
// primitives; the volumes come from exact cross-sections, or from the same
// body built another way (rounded before a notch is cut, a seam turned away).

#include <cmath>
#include <cstddef>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <GeomConvert.hxx>
#include <GeomLib.hxx>
#include <GeomProjLib.hxx>
#include <Geom_BoundedCurve.hxx>
#include <Geom_Circle.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeCustom.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Vec.hxx>

#include "../src/util.hpp"
#include "check.hpp"

namespace test {
namespace {

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The volume integrated adaptively (OCCT's default integration errs on
// the spline faces of walked roundings).
double exact_volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int faces_of(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    ++n;
  }
  return n;
}

// The edge of a shape whose middle lies nearest to p.
TopoDS_Edge edge_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    BRepAdaptor_Curve curve(edge);
    const double d = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2).Distance(p);
    if (d < nearest) {
      nearest = d;
      best = edge;
    }
  }
  return best;
}

// OCCT's fillet of the edges nearest to the points, one radius; the result,
// valid, or a null shape.
TopoDS_Shape occt_fillet(const TopoDS_Shape& shape, const std::vector<gp_Pnt>& near_edges, double radius) {
  BRepFilletAPI_MakeFillet maker(shape);
  for (const gp_Pnt& p : near_edges) {
    maker.Add(radius, edge_near(shape, p));
  }
  maker.Build();
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Shape()).IsValid()) {
    return TopoDS_Shape();
  }
  return maker.Shape();
}

// The cross-section a rounding of radius r removes at a straight edge where
// a plane meets a cylinder of radius R, the cylinder's axis parallel to the
// edge at distance h from the plane, the material inside the cylinder on
// the far side of the plane from the axis (a flat cut off a round bar): the
// region between the plane, the circle and the ball.
double flat_on_round_loss(double R, double h, double r) {
  // In the section: the axis at the origin, the plane at z = h, the
  // material above it. The ball's centre at z = h + r, R - r from the axis.
  const double zc = h + r;
  const double xc = std::sqrt((R - r) * (R - r) - zc * zc);
  const double xe = std::sqrt(R * R - h * h); // the edge
  const double s = R / (R - r);               // the contact on the circle
  const double t2x = xc * s, t2z = zc * s;
  // The polygon centre - contact on the plane - edge - contact on the circle.
  const double px[4] = {xc, xc, xe, t2x}, pz[4] = {zc, h, h, t2z};
  double twice = 0.0;
  for (int i = 0; i < 4; ++i) {
    twice += px[i] * pz[(i + 1) % 4] - px[(i + 1) % 4] * pz[i];
  }
  const double polygon = 0.5 * std::abs(twice);
  // The circle's segment between the edge and the contact on it.
  const double theta = std::atan2(t2z, t2x) - std::atan2(h, xe);
  const double segment = 0.5 * R * R * (theta - std::sin(theta));
  // The ball's sector between its two contacts.
  const double phi = std::acos((h - zc) * (t2z - zc) / (r * r));
  return polygon + segment - 0.5 * r * r * phi;
}

void test_slot_end_beside_a_split_side() {
  // A bar of radius 18.75 along y (65 long) whose side is two faces, split
  // at y = -15: a slot 24 high cuts its end y in [-15, 0], so the slot's
  // back wall lies in the plane of the split. The slot's top and bottom
  // edges on the side rounded with 0.5: each rounding ends against the
  // back wall, its contact on the side on the split edge. The removed
  // section is constant along the 15 of each edge.
  const TopoDS_Shape far =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -65, 0), gp_Dir(0, 1, 0)), 18.75, 50).Shape();
  const TopoDS_Shape near =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -15, 0), gp_Dir(0, 1, 0)), 18.75, 15).Shape();
  const TopoDS_Shape bar = BRepAlgoAPI_Fuse(far, near).Shape();
  const TopoDS_Shape slot = BRepPrimAPI_MakeBox(gp_Pnt(-40, -15, -12), gp_Pnt(40, 1, 12)).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(bar, slot).Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const double x = std::sqrt(18.75 * 18.75 - 12.0 * 12.0);
  const double loss = flat_on_round_loss(18.75, 12.0, 0.5) * 15.0;
  // One edge, the two top edges, all four.
  const std::vector<std::vector<gp_Pnt>> sets = {
      {gp_Pnt(-x, -7.5, 12)},
      {gp_Pnt(-x, -7.5, 12), gp_Pnt(x, -7.5, 12)},
      {gp_Pnt(-x, -7.5, 12), gp_Pnt(x, -7.5, 12), gp_Pnt(-x, -7.5, -12), gp_Pnt(x, -7.5, -12)}};
  for (const std::vector<gp_Pnt>& edges : sets) {
    const TopoDS_Shape rounded = occt_fillet(body, edges, 0.5);
    CHECK(!rounded.IsNull());
    if (!rounded.IsNull()) {
      CHECK_NEAR_TOL(volume_of(rounded), volume_of(body) - loss * static_cast<double>(edges.size()), 1e-9);
      CHECK(faces_of(rounded) == faces_of(body) + static_cast<int>(edges.size()));
    }
  }
}

// A prism of a closed polygon in the (x, y) plane, z from 0 to h.
TopoDS_Shape xy_prism(const std::vector<std::pair<double, double>>& xy, double h) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& [x, y] : xy) {
    polygon.Add(gp_Pnt(x, y, 0));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, h)).Shape();
}

void test_rounding_ends_on_a_pads_split_foot() {
  // A block (x in [0, 20], 15 high) with a pad 0.5 thick on its side
  // x = 0, y in [-15, 15], its ends bevelled at 45 degrees, fused to it, so
  // that the pad's foot and top are faces of their own beside the block's
  // bottom and top. The concave edges where the bevels meet the side are
  // rounded with 30: the ball touches the side and rolls on the pad's front
  // edge (centre 30 in front of the side, sqrt(29.75) beyond the edge).
  // The rounding ends on the block's bottom edge and at the pad's bottom
  // vertex, faces of one plane split in two. The section filled is the
  // triangle side - bevel - contact less the circle's segment over the
  // chord from the contact to the pad's edge.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, -40, 0), gp_Pnt(20, 40, 15)).Shape();
  const TopoDS_Shape pad = xy_prism({{0, -15}, {0, 15}, {-0.5, 14.5}, {-0.5, -14.5}}, 15);
  const TopoDS_Shape body = BRepAlgoAPI_Fuse(block, pad).Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const double c = std::sqrt(29.75);
  const double triangle = 0.5 * 0.5 * (c - 0.5);
  const double theta = 2.0 * std::asin(std::sqrt(0.25 + 29.75) / 60.0);
  const double fill = (triangle - 0.5 * 900.0 * (theta - std::sin(theta))) * 15.0;
  const std::vector<std::vector<gp_Pnt>> sets = {{gp_Pnt(0, 15, 7.5)}, {gp_Pnt(0, 15, 7.5), gp_Pnt(0, -15, 7.5)}};
  for (const std::vector<gp_Pnt>& edges : sets) {
    const TopoDS_Shape rounded = occt_fillet(body, edges, 30.0);
    CHECK(!rounded.IsNull());
    if (!rounded.IsNull()) {
      CHECK_NEAR_TOL(volume_of(rounded), volume_of(body) + fill * static_cast<double>(edges.size()), 1e-9);
      // Each bevel and the edge it shares with the pad's front gone, a
      // rounding added.
      CHECK(faces_of(rounded) == faces_of(body));
    }
  }
}

void test_closed_rounding_rolling_from_a_seam_vertex() {
  // A disc of radius 6.95, 1 high, under a 45-degree bevel to radius 5.95
  // and a neck, fused from a cylinder, a cone and a cylinder (their seams at
  // one angle). The disc's bottom edge rounded with 2: the side and the
  // bevel are too narrow, the ball rolls on the bevel's top edge (centre at
  // radius 3.95, 2 up) all round; the start of the closed rounding lies at
  // that edge's vertex, where the neck's seam starts too. The section
  // removed is the polygon foot - side - bevel less the circle's segment
  // over the quarter arc, revolved (Pappus).
  const gp_Dir up(0, 0, 1);
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), up), 6.95, 1).Shape();
  const TopoDS_Shape bevel = BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 1), up), 6.95, 5.95, 1).Shape();
  const TopoDS_Shape neck = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 2), up), 5.95, 3).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(disc, bevel).Shape(), neck).Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // The polygon (3.95, 0) (6.95, 0) (6.95, 1) (5.95, 2): area and moment
  // about the axis.
  const double pr[4] = {3.95, 6.95, 6.95, 5.95}, pz[4] = {0, 0, 1, 2};
  double twice = 0.0, moment6 = 0.0;
  for (int i = 0; i < 4; ++i) {
    const int j = (i + 1) % 4;
    const double cross = pr[i] * pz[j] - pr[j] * pz[i];
    twice += cross;
    moment6 += (pr[i] + pr[j]) * cross;
  }
  const double polygon_moment = moment6 / 6.0;
  // The segment of the circle of radius 2 round (3.95, 2) over the quarter
  // arc from (3.95, 0) to (5.95, 2).
  const double theta = kPi / 2.0;
  const double segment = 2.0 * (theta - std::sin(theta));
  const double d = 4.0 * 2.0 * std::pow(std::sin(theta / 2.0), 3) / (3.0 * (theta - std::sin(theta)));
  const double segment_moment = segment * (3.95 + d * std::sqrt(0.5));
  const double removed = 2.0 * kPi * (polygon_moment - segment_moment);
  CHECK(twice > 0.0);
  const TopoDS_Shape rounded = occt_fillet(body, {gp_Pnt(-6.95, 0, 0)}, 2.0);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(exact_volume_of(rounded), exact_volume_of(body) - removed, 1e-7);
    // The side and the bevel gone, the rounding added.
    CHECK(faces_of(rounded) == faces_of(body) - 1);
  }
}

void test_plate_edges_rounded_to_a_full_round() {
  // A plate 1.5 thick, its two top edges rounded with 0.75: the contact
  // lines meet in the middle of the top, which vanishes; the two quarter
  // rounds make a half round. Each removes its quarter of the corner
  // square.
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 1.5, 10)).Shape();
  const TopoDS_Shape rounded = occt_fillet(plate, {gp_Pnt(10, 0, 10), gp_Pnt(10, 1.5, 10)}, 0.75);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(volume_of(rounded), 300.0 - 2.0 * fillet_loss(0.75) * 20.0, 1e-9);
    // The top gone, two roundings added.
    CHECK(faces_of(rounded) == faces_of(plate) + 1);
  }
}

// The faces of a shape that lie on the sphere of radius r about c (sampled
// 10 x 10 within their bounds, within tol), and their total area.
int faces_on_sphere(const TopoDS_Shape& shape, const gp_Pnt& c, double r, double tol, double& area) {
  int n = 0;
  area = 0.0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    const TopoDS_Face& face = TopoDS::Face(e.Current());
    BRepAdaptor_Surface s(face);
    double u1 = 0.0, u2 = 0.0, v1 = 0.0, v2 = 0.0;
    BRepTools::UVBounds(face, u1, u2, v1, v2);
    bool on = true;
    for (int i = 0; i <= 10 && on; ++i) {
      for (int j = 0; j <= 10 && on; ++j) {
        on = std::abs(s.Value(u1 + (u2 - u1) * i / 10, v1 + (v2 - v1) * j / 10).Distance(c) - r) <= tol;
      }
    }
    if (on) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(face, props);
      area += props.Mass();
      ++n;
    }
  }
  return n;
}

// The material a rounding of radius r takes off (or adds) where it runs
// round a corner rounded with r: a quarter column r high less the ball's
// eighth.
double ball_corner_loss(double r) { return kPi * r * r * r / 4.0 - kPi * r * r * r / 6.0; }

void test_top_rounded_past_a_corner_of_the_balls_radius() {
  // A box 20 x 20 x 10 whose vertical edge at (20, 20) is rounded with 2,
  // stored as splines (so that the rounding is walked, not analytic); then
  // the top edges at that corner (one tangent chain: an edge, the arc, an
  // edge) rounded with 2. Round the arc the ball's centre stays on the
  // corner's axis, 2 below the top: its contact on the top stands still
  // while the other sweeps round the arc, and the blend is an eighth of the
  // ball's sphere. The two straight roundings take their sections along
  // 18 each, the corner a quarter column less the ball's eighth.
  const double r = 2.0;
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(20, 20, 10).Shape();
  BRepFilletAPI_MakeFillet corner(box);
  corner.Add(r, edge_near(box, gp_Pnt(20, 20, 5)));
  corner.Build();
  CHECK(corner.IsDone());
  if (!corner.IsDone()) {
    return;
  }
  const TopoDS_Shape body = BRepBuilderAPI_NurbsConvert(corner.Shape()).Shape();
  const double before = 4000.0 - 10.0 * fillet_loss(r);
  CHECK_NEAR_TOL(exact_volume_of(body), before, 1e-8);
  const TopoDS_Shape rounded = occt_fillet(body, {gp_Pnt(10, 20, 10)}, r);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(exact_volume_of(rounded), before - 36.0 * fillet_loss(r) - ball_corner_loss(r), 1e-8);
    double area = 0.0;
    CHECK(faces_on_sphere(rounded, gp_Pnt(18, 18, 8), r, 1e-4, area) == 1);
    CHECK_NEAR_TOL(area, kPi * r * r / 2.0, 1e-5);
  }
}

void test_closed_top_rounded_past_four_corners() {
  // The same box with all four vertical edges rounded with 2: the top's
  // outline is one closed tangent chain, rounded with 2, the ball turning
  // about a standing contact at each corner.
  const double r = 2.0;
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(20, 20, 10).Shape();
  BRepFilletAPI_MakeFillet corners(box);
  for (const gp_Pnt& p : {gp_Pnt(20, 20, 5), gp_Pnt(0, 20, 5), gp_Pnt(0, 0, 5), gp_Pnt(20, 0, 5)}) {
    corners.Add(r, edge_near(box, p));
  }
  corners.Build();
  CHECK(corners.IsDone());
  if (!corners.IsDone()) {
    return;
  }
  const TopoDS_Shape body = BRepBuilderAPI_NurbsConvert(corners.Shape()).Shape();
  const double before = 4000.0 - 40.0 * fillet_loss(r);
  const TopoDS_Shape rounded = occt_fillet(body, {gp_Pnt(10, 20, 10)}, r);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(exact_volume_of(rounded), before - 64.0 * fillet_loss(r) - 4.0 * ball_corner_loss(r), 1e-8);
    for (const gp_Pnt& c : {gp_Pnt(18, 18, 8), gp_Pnt(2, 18, 8), gp_Pnt(2, 2, 8), gp_Pnt(18, 2, 8)}) {
      double area = 0.0;
      CHECK(faces_on_sphere(rounded, c, r, 1e-4, area) == 1);
      CHECK_NEAR_TOL(area, kPi * r * r / 2.0, 1e-5);
    }
  }
}

void test_pocket_floor_rounded_past_its_corners() {
  // A pocket 20 x 20, 10 deep in a block 40 x 40 x 20, its inner vertical
  // edges rounded with 2, stored as splines; then the floor's outline
  // rounded with 2 (concave): the ball turns about a standing contact on
  // the floor at each corner. Each rounding adds what it would take off a
  // convex edge.
  const double r = 2.0;
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, 0), gp_Pnt(20, 20, 20)).Shape();
  const TopoDS_Shape cut = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, 10), gp_Pnt(10, 10, 30)).Shape();
  const TopoDS_Shape pocket = BRepAlgoAPI_Cut(block, cut).Shape();
  BRepFilletAPI_MakeFillet corners(pocket);
  for (const gp_Pnt& p : {gp_Pnt(10, 10, 15), gp_Pnt(-10, 10, 15), gp_Pnt(-10, -10, 15), gp_Pnt(10, -10, 15)}) {
    corners.Add(r, edge_near(pocket, p));
  }
  corners.Build();
  CHECK(corners.IsDone());
  if (!corners.IsDone()) {
    return;
  }
  const TopoDS_Shape body = BRepBuilderAPI_NurbsConvert(corners.Shape()).Shape();
  const double before = 28000.0 + 40.0 * fillet_loss(r);
  const TopoDS_Shape rounded = occt_fillet(body, {gp_Pnt(0, 10, 10)}, r);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(exact_volume_of(rounded), before + 64.0 * fillet_loss(r) + 4.0 * ball_corner_loss(r), 1e-8);
    for (const gp_Pnt& c : {gp_Pnt(8, 8, 12), gp_Pnt(-8, 8, 12), gp_Pnt(-8, -8, 12), gp_Pnt(8, -8, 12)}) {
      double area = 0.0;
      CHECK(faces_on_sphere(rounded, c, r, 1e-4, area) == 1);
      CHECK_NEAR_TOL(area, kPi * r * r / 2.0, 1e-5);
    }
  }
}

// A V notch through the rim of a disc of radius 15 at its bottom (y = -15),
// 0.4 wide at the rim and 0.1 deep in the middle, through the whole height.
TopoDS_Shape rim_notch() {
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& [x, y] : std::vector<std::pair<double, double>>{{0.3, -15.6}, {0.15, -14.9}, {-0.15, -14.9}, {-0.3, -15.6}}) {
    polygon.Add(gp_Pnt(x, y, -1));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 7)).Shape();
}

// The point of the circle of radius 15 at the angle (degrees), at the top.
gp_Pnt on_top_rim(double degrees) {
  return gp_Pnt(15.0 * std::cos(degrees * kPi / 180.0), 15.0 * std::sin(degrees * kPi / 180.0), 5.0);
}

int torus_faces(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    n += BRepAdaptor_Surface(TopoDS::Face(e.Current())).GetType() == GeomAbs_Torus ? 1 : 0;
  }
  return n;
}

void test_rim_rounded_across_a_notch() {
  // A disc of radius 15, 5 high, cut by the plane y = 10 (a D), with a
  // notch in its rim at the bottom: the top's arc is two edges, from the
  // chord's ends to the notch, rounded with 0.3. The rounding's contact on
  // the top passes above the notch (0.3 in, the notch 0.1 deep): the ball
  // rolls on over it, the torus runs on and the notch's faces cut it, and
  // the two edges' blends are one torus face. The same body as the D's whole
  // arc rounded first and the notch cut after.
  const double r = 0.3;
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 15, 5).Shape();
  const TopoDS_Shape d = BRepAlgoAPI_Cut(disc, BRepPrimAPI_MakeBox(gp_Pnt(-20, 10, -1), gp_Pnt(20, 20, 6)).Shape()).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(d, rim_notch()).Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const TopoDS_Shape whole = occt_fillet(d, {on_top_rim(249)}, r);
  CHECK(!whole.IsNull());
  const TopoDS_Shape rounded = occt_fillet(body, {on_top_rim(203.5), on_top_rim(315.5)}, r);
  CHECK(!rounded.IsNull());
  if (!whole.IsNull() && !rounded.IsNull()) {
    const TopoDS_Shape expected = BRepAlgoAPI_Cut(whole, rim_notch()).Shape();
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(expected), 1e-9);
    CHECK(faces_of(rounded) == faces_of(expected));
    CHECK(torus_faces(rounded) == torus_faces(expected));
  }
}

// The D's arc on one side of the notch rounded, carried across the notch
// (port patch 0260, mitcad#307). With 0.3 the notch (0.1 deep, 0.34 wide at
// the rim) lies within the rounding's contact on the top and within the
// ball: the arc beyond it is added as a contour of its own, the two meet
// over the notch (0241), and the body is the one with both arcs given. With
// 0.15 the ball is narrower than the notch, with 0.08 the contact lies
// above the notch's bottom: nothing is carried. Without the carry the
// rounding ends at the notch and the result is invalid.
void test_rounding_carried_across_a_notch() {
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 15, 5).Shape();
  const TopoDS_Shape d = BRepAlgoAPI_Cut(disc, BRepPrimAPI_MakeBox(gp_Pnt(-20, 10, -1), gp_Pnt(20, 20, 6)).Shape()).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(d, rim_notch()).Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const TopoDS_Shape both = occt_fillet(body, {on_top_rim(203.5), on_top_rim(315.5)}, 0.3);
  CHECK(!both.IsNull());
  const TopoDS_Shape whole = occt_fillet(d, {on_top_rim(249)}, 0.3);
  CHECK(!whole.IsNull());
  BRepFilletAPI_MakeFillet carried(body);
  const TopoDS_Edge given = edge_near(body, on_top_rim(203.5));
  carried.Add(0.3, given);
  CHECK(carried.CarryChainsAcrossNotches() == 1);
  // (The contour added comes before the one given, whose face the joined
  // blends then are.)
  CHECK(carried.NbContours() == 2);
  CHECK(carried.Contour(edge_near(body, on_top_rim(315.5))) == 1);
  CHECK(carried.Contour(given) == 2);
  carried.Build();
  CHECK(carried.IsDone());
  if (!both.IsNull() && !whole.IsNull() && carried.IsDone()) {
    const TopoDS_Shape rounded = carried.Shape();
    const TopoDS_Shape expected = BRepAlgoAPI_Cut(whole, rim_notch()).Shape();
    CHECK(BRepCheck_Analyzer(rounded).IsValid());
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(expected), 1e-8);
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(both), 1e-8);
    CHECK(faces_of(rounded) == faces_of(both));
    CHECK(torus_faces(rounded) == torus_faces(both));
    int given_tori = 0;
    for (const TopoDS_Shape& f : carried.Generated(given)) {
      given_tori += f.ShapeType() == TopAbs_FACE &&
                            BRepAdaptor_Surface(TopoDS::Face(f)).GetType() == GeomAbs_Torus
                        ? 1
                        : 0;
    }
    CHECK(given_tori >= 1);
  }
  BRepFilletAPI_MakeFillet alone(body);
  alone.Add(0.3, edge_near(body, on_top_rim(203.5)));
  alone.Build();
  CHECK(!alone.IsDone() || !BRepCheck_Analyzer(alone.Shape()).IsValid());
  for (const double r : {0.15, 0.08}) {
    BRepFilletAPI_MakeFillet narrow(body);
    narrow.Add(r, edge_near(body, on_top_rim(203.5)));
    CHECK(narrow.CarryChainsAcrossNotches() == 0);
    CHECK(narrow.NbContours() == 1);
  }
}

void test_closed_rim_rounded_across_a_notch() {
  // The whole disc with the notch: the top's rim is one arc from the notch
  // round to it, rounded with 0.3; its two ends meet over the notch. The
  // same body as the disc's rim rounded first and the notch cut after, but
  // for the section where the ends meet (a face closed on itself needs one).
  const double r = 0.3;
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 15, 5).Shape();
  const TopoDS_Shape body = BRepAlgoAPI_Cut(disc, rim_notch()).Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const TopoDS_Shape whole = occt_fillet(disc, {on_top_rim(180)}, r);
  CHECK(!whole.IsNull());
  const TopoDS_Shape rounded = occt_fillet(body, {on_top_rim(135)}, r);
  CHECK(!rounded.IsNull());
  if (!whole.IsNull() && !rounded.IsNull()) {
    const TopoDS_Shape expected = BRepAlgoAPI_Cut(whole, rim_notch()).Shape();
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(expected), 1e-9);
    CHECK(faces_of(rounded) == faces_of(expected) + 1);
  }
}

// A prism 5 high whose outline runs on a circle of radius 15 from (0, 15)
// to (-15, 0), turns there by 16.5 degrees into a convex arc of radius 1
// (60 degrees), then straight back round; its sides as cylinders and
// planes. The small arc's circle has its seam at `seam` radians before the
// arc's start.
TopoDS_Shape kinked_outline_prism(double seam) {
  const double turn = 16.5 * kPi / 180.0, sweep = kPi / 3.0;
  const gp_Pnt S(-15, 0, 0);
  const gp_Pnt C(S.X() + std::cos(turn), S.Y() + std::sin(turn), 0);
  const double start = std::atan2(S.Y() - C.Y(), S.X() - C.X());
  const double x0 = start - seam;
  const occ::handle<Geom_Circle> big = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), 15);
  const occ::handle<Geom_Circle> small =
      new Geom_Circle(gp_Ax2(C, gp_Dir(0, 0, 1), gp_Dir(std::cos(x0), std::sin(x0), 0)), 1);
  const gp_Pnt E = small->Value(seam + sweep);
  const double out = std::atan2(-std::cos(turn), std::sin(turn)) + sweep;
  const gp_Pnt Q(E.X() + 8 * std::cos(out), E.Y() + 8 * std::sin(out), 0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(big, gp_Pnt(0, 15, 0), S).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(small, seam, seam + sweep).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(E, Q).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(Q, gp_Pnt(10, Q.Y(), 0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(10, Q.Y(), 0), gp_Pnt(10, 15, 0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(10, 15, 0), gp_Pnt(0, 15, 0)).Edge());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return ShapeCustom::SweptToElementary(BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 5)).Shape());
}

void test_extension_rolling_on_past_a_seam() {
  // The kinked outline's two arcs rounded with 0.5 (two roundings meeting
  // at the kink). Past the small arc's start the blend rolls on the small
  // cylinder's own surface for the corner, here across its circle's seam,
  // 0.0758 before the arc: the same body as with the seam far away.
  const double turn = 16.5 * kPi / 180.0;
  const gp_Pnt C(-15 + std::cos(turn), std::sin(turn), 0);
  const double start = std::atan2(-C.Y(), -15 - C.X());
  const std::vector<gp_Pnt> edges = {gp_Pnt(15 * std::cos(3 * kPi / 4), 15 * std::sin(3 * kPi / 4), 5),
                                     gp_Pnt(C.X() + std::cos(start + kPi / 6), C.Y() + std::sin(start + kPi / 6), 5)};
  const TopoDS_Shape near = occt_fillet(kinked_outline_prism(0.0758), edges, 0.5);
  const TopoDS_Shape far = occt_fillet(kinked_outline_prism(kPi), edges, 0.5);
  CHECK(!near.IsNull());
  CHECK(!far.IsNull());
  if (!near.IsNull() && !far.IsNull()) {
    CHECK_NEAR_TOL(volume_of(near), volume_of(far), 1e-9);
    CHECK(faces_of(near) == faces_of(far));
  }
}

// A body fused from boxes, its faces of one plane merged.
TopoDS_Shape fused_boxes(const std::vector<std::pair<gp_Pnt, gp_Pnt>>& boxes) {
  TopoDS_Shape body = BRepPrimAPI_MakeBox(boxes[0].first, boxes[0].second).Shape();
  for (std::size_t i = 1; i < boxes.size(); ++i) {
    body = BRepAlgoAPI_Fuse(body, BRepPrimAPI_MakeBox(boxes[i].first, boxes[i].second).Shape()).Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(body, true, true, true);
  unify.Build();
  return unify.Shape();
}

void test_rib_rounded_to_a_full_round_out_of_a_plate() {
  // A rib 2 wide and 10 long standing out of a plate's side, its top one
  // face with the plate's top; the rib's two top edges rounded with 1. The
  // contact lines on the top coincide along the rib's middle: the blends
  // meet there and the top vanishes over the rib, but goes on over the
  // plate, cut off at the rib's root where the corners end against the
  // plate's side (its top edge goes on across the root).
  const TopoDS_Shape body = fused_boxes(
      {{gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 5)}, {gp_Pnt(20, 9, 0), gp_Pnt(30, 11, 5)}});
  const TopoDS_Shape rounded = occt_fillet(body, {gp_Pnt(25, 9, 5), gp_Pnt(25, 11, 5)}, 1.0);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(body) - 2.0 * fillet_loss(1.0) * 10.0, 1e-9);
    // Two roundings added; the top no longer reaches over the rib.
    CHECK(faces_of(rounded) == faces_of(body) + 2);
    GProp_GProps props;
    BRepGProp::SurfaceProperties(rounded, props);
    GProp_GProps before;
    BRepGProp::SurfaceProperties(body, before);
    // The top loses the rib's 20 and the rib's sides 10 each; the plate's
    // side gains the two caps at the root that the rib's end loses at the
    // tip; the two quarter rounds add pi x 10.
    CHECK_NEAR_TOL(props.Mass(), before.Mass() - 40.0 + kPi * 10.0, 1e-9);
  }
  // The same rib bridging two plates: the top goes on past both ends.
  const TopoDS_Shape bridge = fused_boxes({{gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 5)},
                                           {gp_Pnt(30, 0, 0), gp_Pnt(50, 20, 5)},
                                           {gp_Pnt(20, 9, 0), gp_Pnt(30, 11, 5)}});
  const TopoDS_Shape bridged = occt_fillet(bridge, {gp_Pnt(25, 9, 5), gp_Pnt(25, 11, 5)}, 1.0);
  CHECK(!bridged.IsNull());
  if (!bridged.IsNull()) {
    CHECK_NEAR_TOL(volume_of(bridged), volume_of(bridge) - 2.0 * fillet_loss(1.0) * 10.0, 1e-9);
  }
}

void test_rib_rounded_to_a_full_round_out_of_a_ring() {
  // A ring (radii 15 and 20, 5 high) with a rib 2 wide standing out of its
  // outer side to x = 30, the rib's top one face with the ring's; the rib's
  // top edges rounded with 1. The corners at the root end against the
  // ring's outer cylinder, whose top circle goes on across the root. The
  // body is the ring fused with the rib rounded on its own (its top a strip
  // that vanishes whole, 0153).
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 20, 5).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 15, 7).Shape();
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(outer, inner).Shape();
  const TopoDS_Shape rib = BRepPrimAPI_MakeBox(gp_Pnt(18, -1, 0), gp_Pnt(30, 1, 5)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(ring, rib).Shape(), true, true, true);
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  const TopoDS_Shape rounded = occt_fillet(body, {gp_Pnt(25, -1, 5), gp_Pnt(25, 1, 5)}, 1.0);
  const TopoDS_Shape rib_alone = occt_fillet(rib, {gp_Pnt(24, -1, 5), gp_Pnt(24, 1, 5)}, 1.0);
  CHECK(!rounded.IsNull());
  CHECK(!rib_alone.IsNull());
  if (!rounded.IsNull() && !rib_alone.IsNull()) {
    CHECK_NEAR_TOL(exact_volume_of(rounded), exact_volume_of(BRepAlgoAPI_Fuse(ring, rib_alone).Shape()), 1e-9);
    // The ring's faces, the rib's two sides and end, two roundings.
    CHECK(faces_of(rounded) == faces_of(body) + 2);
  }
}

void test_slot_rounded_to_a_full_round_into_a_floor() {
  // Concave: a slot 2 wide and 10 long sunk into a block's side wall, its
  // floor one face with the floor of the step in front of the wall; the
  // slot's two floor edges rounded with 1. The contact lines on the floor
  // coincide along the slot's middle and the floor vanishes in the slot,
  // cut off at the slot's mouth.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(30, 20, 10)).Shape();
  const TopoDS_Shape step = BRepPrimAPI_MakeBox(gp_Pnt(-1, -1, 5), gp_Pnt(15, 21, 11)).Shape();
  const TopoDS_Shape slot = BRepPrimAPI_MakeBox(gp_Pnt(10, 9, 5), gp_Pnt(25, 11, 11)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(block, step).Shape(), slot).Shape(), true,
                                     true, true);
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  const TopoDS_Shape rounded = occt_fillet(body, {gp_Pnt(20, 9, 5), gp_Pnt(20, 11, 5)}, 1.0);
  CHECK(!rounded.IsNull());
  if (!rounded.IsNull()) {
    CHECK_NEAR_TOL(volume_of(rounded), volume_of(body) + 2.0 * fillet_loss(1.0) * 10.0, 1e-9);
  }
}

// A block 20 x 12 x 10 whose two long sides are concave arcs of radius 20,
// cut in by `cut` at the middle (the top 12 - 2 cut wide there).
TopoDS_Shape waisted_block(double cut) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 12, 10)).Shape();
  const TopoDS_Shape c1 =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, cut - 20, -1), gp_Dir(0, 0, 1)), 20, 12).Shape();
  const TopoDS_Shape c2 =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, 32 - cut, -1), gp_Dir(0, 0, 1)), 20, 12).Shape();
  return BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(block, c1).Shape(), c2).Shape();
}

// The body with edges e1 and e2 rounded (r1, r2) one at a time: the common
// of the two (convex roundings) or their fuse (concave ones), which is the
// two roundings cut by each other.
TopoDS_Shape rounded_apart(const TopoDS_Shape& body, const gp_Pnt& e1, double r1, const gp_Pnt& e2, double r2,
                           bool concave) {
  const TopoDS_Shape one = occt_fillet(body, {e1}, r1);
  const TopoDS_Shape two = occt_fillet(body, {e2}, r2);
  if (one.IsNull() || two.IsNull()) {
    return TopoDS_Shape();
  }
  return concave ? BRepAlgoAPI_Fuse(one, two).Shape() : BRepAlgoAPI_Common(one, two).Shape();
}

void test_roundings_cut_by_each_other_where_the_top_narrows() {
  // A block whose top narrows from 10.4 to 5.8 in the middle (its long
  // sides concave arcs), both top edges rounded with 4: the contacts on
  // the top cross each other twice, where the top is 8 wide. Between the
  // crossings the two tori cut each other, and the top is left in two
  // pieces, one at each end; the body is the block rounded at each edge
  // alone, the two in common.
  struct Case {
    double cut, r1, r2;
    bool splines;
  };
  for (const Case& c : {Case{3.1, 4.0, 4.0, false}, Case{3.5, 4.0, 3.0, false}, Case{3.5, 4.0, 4.0, true}}) {
    TopoDS_Shape body = waisted_block(c.cut);
    if (c.splines) {
      // Stored as splines: the roundings are walked.
      body = BRepBuilderAPI_NurbsConvert(body).Shape();
    }
    const gp_Pnt e1(10, c.cut, 10), e2(10, 12 - c.cut, 10);
    BRepFilletAPI_MakeFillet maker(body);
    maker.Add(c.r1, edge_near(body, e1));
    maker.Add(c.r2, edge_near(body, e2));
    maker.Build();
    const TopoDS_Shape both =
        maker.IsDone() && BRepCheck_Analyzer(maker.Shape()).IsValid() ? maker.Shape() : TopoDS_Shape();
    const TopoDS_Shape apart = rounded_apart(body, e1, c.r1, e2, c.r2, false);
    CHECK(!both.IsNull());
    CHECK(!apart.IsNull());
    if (!both.IsNull() && !apart.IsNull()) {
      // (Walked roundings on splines: their approximation.)
      CHECK_NEAR_TOL(exact_volume_of(both), exact_volume_of(apart), c.splines ? 1e-6 : 1e-9);
      // The block's six faces, the top in two, two roundings.
      CHECK(faces_of(both) == 9);
    }
  }
}

void test_concave_roundings_cut_by_each_other_where_a_channel_narrows() {
  // Concave: a channel 12 wide and 5 deep whose walls bulge in to 5 apart
  // in the middle (arcs of radius 20), its floor edges rounded with 4: the
  // contacts on the floor cross twice, and between the crossings the two
  // roundings fill the floor up to where they cut each other. The body is
  // the channel rounded at each edge alone, the two fused.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, -10, 0), gp_Pnt(20, 22, 10)).Shape();
  const TopoDS_Shape channel = BRepPrimAPI_MakeBox(gp_Pnt(-1, 0, 5), gp_Pnt(21, 12, 11)).Shape();
  const TopoDS_Shape w1 =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, 3.5 - 20, 4), gp_Dir(0, 0, 1)), 20, 8).Shape();
  const TopoDS_Shape w2 =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10, 8.5 + 20, 4), gp_Dir(0, 0, 1)), 20, 8).Shape();
  const TopoDS_Shape walls =
      BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(BRepAlgoAPI_Cut(block, channel).Shape(), w1).Shape(), w2).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Common(walls, block).Shape(), true, true, true);
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  const gp_Pnt e1(10, 3.5, 5), e2(10, 8.5, 5);
  const TopoDS_Shape rounded = occt_fillet(body, {e1, e2}, 4.0);
  const TopoDS_Shape apart = rounded_apart(body, e1, 4.0, e2, 4.0, true);
  CHECK(!rounded.IsNull());
  CHECK(!apart.IsNull());
  if (!rounded.IsNull() && !apart.IsNull()) {
    CHECK_NEAR_TOL(exact_volume_of(rounded), exact_volume_of(apart), 1e-9);
  }
}

// A rod along x (radius 10, from x = -10 to 30) whose seam runs along
// `seam`, with a rib 4 wide and 20 long fused on its top.
TopoDS_Shape rod_with_rib(const gp_Dir& seam) {
  const TopoDS_Shape rod = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-10, 0, 0), gp_Dir(1, 0, 0), seam), 10, 40).Shape();
  const TopoDS_Shape rib = BRepPrimAPI_MakeBox(gp_Pnt(0, -2, 0), gp_Pnt(20, 2, 12)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(rod, rib).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

void test_rib_foot_rounded_across_the_rods_seam() {
  // Concave: a rib's foot on a rod rounded with 1. At the rib's ends the
  // end faces grow down to the rounding's contact on the rod, 0.73 beyond
  // the foot, prolonging their edges with the rod; the rod's seam runs
  // between the foot and the contact (at y = 2.4), so the prolonged edge
  // is cut at the seam. The result is the one with the seam turned away
  // to the rod's bottom (0300: the pieces' curves on the end face were
  // taken from the rod).
  const double s = 0.24;
  const TopoDS_Shape across = rod_with_rib(gp_Dir(0, s, std::sqrt(1 - s * s)));
  const TopoDS_Shape away = rod_with_rib(gp_Dir(0, 0, -1));
  const double foot = std::sqrt(100.0 - 4.0);
  for (const std::vector<gp_Pnt>& edges :
       {std::vector<gp_Pnt>{gp_Pnt(10, 2, foot)}, std::vector<gp_Pnt>{gp_Pnt(10, 2, foot), gp_Pnt(10, -2, foot)}}) {
    const TopoDS_Shape rounded = occt_fillet(across, edges, 1.0);
    const TopoDS_Shape reference = occt_fillet(away, edges, 1.0);
    CHECK(!rounded.IsNull());
    CHECK(!reference.IsNull());
    if (!rounded.IsNull() && !reference.IsNull()) {
      CHECK_NEAR_TOL(exact_volume_of(rounded), exact_volume_of(reference), 1e-9);
      CHECK(exact_volume_of(rounded) > exact_volume_of(across));
      CHECK(faces_of(rounded) == faces_of(reference));
    }
  }
}


// A tube (bore of radius 15, outer radius 18.5, z 10 to 15) with a bar
// x -1.5..1.5, z 10..13.5 across it, fused; the bore's seam at the angle of
// the bar's corner (1.5, 14.92) turned by `turn`. `copied`: the fused body
// copied, whose edge along the seam's line below the corner has one curve
// on the bore (as fused it keeps the seam's two).
TopoDS_Shape bar_in_a_tube(double turn, bool copied) {
  const double y = std::sqrt(225.0 - 2.25);
  const double a = std::atan2(y, 1.5) + turn;
  const gp_Ax2 axis(gp_Pnt(0, 0, 10), gp_Dir(0, 0, 1), gp_Dir(std::cos(a), std::sin(a), 0));
  const TopoDS_Shape tube = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(axis, 18.5, 5).Shape(),
                                            BRepPrimAPI_MakeCylinder(axis, 15, 5).Shape())
                                .Shape();
  BRepAlgoAPI_Fuse fuse(tube, BRepPrimAPI_MakeBox(gp_Pnt(-1.5, -16, 10), gp_Pnt(1.5, 16, 13.5)).Shape());
  fuse.SimplifyResult();
  return copied ? BRepBuilderAPI_Copy(fuse.Shape()).Shape() : fuse.Shape();
}

void test_bar_top_rounded_into_a_bore_along_its_seam() {
  // The bar's top edge at x = 1.5 rounded with 1: at y = 14.92 the
  // rounding ends on the bore, whose seam runs up from the corner along
  // the line of the bar's edge on the bore. The rounding uncovers the bore
  // on both sides of that line from the rounding's contact on the bar's
  // side (z = 12.5) up to the corner: the seam goes on down to there (port
  // patch 0690; before, the bore's face was unorientable). The result is
  // the one with the seam turned away.
  const std::vector<gp_Pnt> edges{gp_Pnt(1.5, 0, 13.5)};
  const TopoDS_Shape reference = occt_fillet(bar_in_a_tube(0.1, true), edges, 1.0);
  CHECK(!reference.IsNull());
  for (const bool copied : {false, true}) {
    const TopoDS_Shape body = bar_in_a_tube(0.0, copied);
    const TopoDS_Shape rounded = occt_fillet(body, edges, 1.0);
    CHECK(!rounded.IsNull());
    if (!rounded.IsNull() && !reference.IsNull()) {
      CHECK_NEAR_TOL(exact_volume_of(rounded), exact_volume_of(reference), 1e-7);
      CHECK(exact_volume_of(rounded) < exact_volume_of(body));
      CHECK(faces_of(rounded) == faces_of(reference));
    }
  }
}

// A disc (radius 23, z 10 to 30) with a recess from below, a cone from
// radius 17 at z = 10 to 15 at z = 12 and a bore of 15 above, crossed by a
// rib along y (|x| <= 1.5) whose bottom is the disc's bottom: flat for
// |x| < a, rounded with 1 up to 45-degree slopes that meet the rib's sides.
// The roundings meet the slopes where a rounding of 1 of the recess's rim
// has its contact on the cone. `overshoot`: the curves of the edges
// between the cone and the rib's roundings carried that far past their
// vertices with the slopes (stopped that far short of them where it is
// negative), those vertices' tolerance 2.4e-4 covering it (as an imported
// body's edges may be).
TopoDS_Shape rib_across_a_recess(double overshoot) {
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 10), gp_Dir(0, 0, 1)), 23, 20).Shape();
  const TopoDS_Shape recess =
      BRepAlgoAPI_Fuse(BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 10), gp_Dir(0, 0, 1)), 17, 15, 2).Shape(),
                       BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 12), gp_Dir(0, 0, 1)), 15, 8).Shape())
          .Shape();
  const double a = 0.5 - std::tan(kPi / 8), s = std::sqrt(0.5);
  const auto at = [](double x, double z) { return gp_Pnt(x, -30, z); };
  const auto arc = [&](double x0, double x1, double sign) {
    // From (x0, z0) to (x1, z1) round (sign a, 11), through the middle.
    const gp_Pnt c(sign * a, -30, 11);
    const gp_Pnt p0 = at(x0, x0 == sign * a ? 10 : 11 - s), p1 = at(x1, x1 == sign * a ? 10 : 11 - s);
    gp_Vec m = gp_Vec(c, p0).Normalized() + gp_Vec(c, p1).Normalized();
    m.Normalize();
    return BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p0, c.Translated(m), p1).Value()).Edge();
  };
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(at(-a, 10), at(a, 10)).Edge());
  wire.Add(arc(a, a + s, 1));
  wire.Add(BRepBuilderAPI_MakeEdge(at(a + s, 11 - s), at(1.5, 11)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(1.5, 11), at(1.5, 25)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(1.5, 25), at(-1.5, 25)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(-1.5, 25), at(-1.5, 11)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(-1.5, 11), at(-a - s, 11 - s)).Edge());
  wire.Add(arc(-a - s, -a, -1));
  const TopoDS_Shape rib =
      BRepAlgoAPI_Common(BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face(), gp_Vec(0, 60, 0)).Shape(),
                         recess)
          .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(BRepAlgoAPI_Cut(disc, recess).Shape(), rib).Shape(), true, true,
                                     true);
  unify.Build();
  TopoDS_Shape body = unify.Shape();
  if (overshoot == 0) {
    return body;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of_edge;
  TopExp::MapShapesAndUniqueAncestors(body, TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
  const occ::handle<ShapeBuild_ReShape> reshape = new ShapeBuild_ReShape;
  for (int i = 1; i <= faces_of_edge.Extent(); ++i) {
    const TopoDS_Edge edge = TopoDS::Edge(faces_of_edge.FindKey(i));
    double f = 0, l = 0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, f, l);
    const BRepAdaptor_Curve adaptor(edge);
    if (adaptor.GetType() == GeomAbs_Line || adaptor.GetType() == GeomAbs_Circle || faces_of_edge(i).Extent() != 2) {
      continue;
    }
    const bool at_last = std::abs(curve->Value(l).Z() - (11 - s)) < 1e-6;
    if ((!at_last && std::abs(curve->Value(f).Z() - (11 - s)) >= 1e-6) ||
        std::min(curve->Value(f).Z(), curve->Value(l).Z()) > 10 + 1e-6) {
      continue;
    }
    gp_Pnt end;
    gp_Vec along;
    curve->D1(at_last ? l : f, end, along);
    // (Stopped short: the curve trimmed by that length.)
    const double cut = overshoot < 0 ? -overshoot / along.Magnitude() : 0.0;
    occ::handle<Geom_BoundedCurve> longer = GeomConvert::CurveToBSplineCurve(
        new Geom_TrimmedCurve(curve, at_last ? f : f + cut, at_last ? l - cut : l));
    along.Normalize();
    if (overshoot > 0) {
      GeomLib::ExtendCurveToPoint(longer, end.Translated(along * (at_last ? overshoot : -overshoot)), 1, at_last);
    }
    BRep_Builder builder;
    TopoDS_Edge carried;
    builder.MakeEdge(carried, longer, 2.4e-4);
    TopoDS_Vertex v1, v2;
    TopExp::Vertices(edge, v1, v2);
    builder.Add(carried, v1.Oriented(TopAbs_FORWARD));
    builder.Add(carried, v2.Oriented(TopAbs_REVERSED));
    builder.Range(carried, longer->FirstParameter(), longer->LastParameter());
    for (const TopoDS_Shape& face : faces_of_edge(i)) {
      double tolerance = 1e-7;
      builder.UpdateEdge(carried,
                         GeomProjLib::Curve2d(longer, longer->FirstParameter(), longer->LastParameter(),
                                              BRep_Tool::Surface(TopoDS::Face(face)), tolerance),
                         TopoDS::Face(face), 2.4e-4);
    }
    builder.UpdateVertex(at_last ? v2 : v1, 2.4e-4);
    reshape->Replace(edge.Oriented(TopAbs_FORWARD), carried.Oriented(TopAbs_FORWARD));
  }
  return reshape->Apply(body);
}

// The recess's rim on the disc's bottom rounded with 1, or a null shape.
TopoDS_Shape round_recess_rim(const TopoDS_Shape& body) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - 17) < 1e-7 &&
        std::abs(curve.Circle().Location().Z() - 10) < 1e-7) {
      fillet.Add(1.0, TopoDS::Edge(e.Current()));
    }
  }
  if (fillet.NbContours() != 2) {
    return TopoDS_Shape();
  }
  fillet.Build();
  if (!fillet.IsDone() || !BRepCheck_Analyzer(fillet.Shape()).IsValid()) {
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

void test_recess_rim_rounded_up_to_a_rib_across_it() {
  // Each half of the rim ends at the rib, on the rib's bottom roundings,
  // which the disc's bottom continues tangentially (OnSame: the rounding's
  // contact on the bottom runs on until it meets the line where the rib's
  // rounding leaves the bottom). That contact is a circle round the axis:
  // it touches the rounding's surface at both of its ends, and the
  // intersection gave the far end's touch, a diameter away (port patch
  // 0691: of it and the contact's crossing with the line, the one nearer
  // the corner). The ends lie where the contact, of radius 17 + tan(pi/8),
  // crosses the lines x = +-a.
  const TopoDS_Shape body = rib_across_a_recess(0);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const TopoDS_Shape rounded = round_recess_rim(body);
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    return;
  }
  CHECK(faces_of(rounded) == faces_of(body) + 2);
  const double a = 0.5 - std::tan(kPi / 8), r = 17 + std::tan(kPi / 8), y = std::sqrt(r * r - a * a);
  for (const gp_Pnt& p : {gp_Pnt(a, y, 10), gp_Pnt(-a, y, 10), gp_Pnt(a, -y, 10), gp_Pnt(-a, -y, 10)}) {
    bool found = false;
    for (TopExp_Explorer e(rounded, TopAbs_VERTEX); e.More() && !found; e.Next()) {
      found = BRep_Tool::Pnt(TopoDS::Vertex(e.Current())).Distance(p) < 1e-6;
    }
    CHECK(found);
  }
  // The rim's cross-section tan(pi/8) - pi/8 round the axis less the rib's
  // width.
  const double removed = exact_volume_of(body) - exact_volume_of(rounded);
  CHECK(removed > 2.25 && removed < 2.30);

  // The same with the edges between the cone and the rib's roundings
  // carried 2e-5 past their vertices with the slopes, within those
  // vertices' tolerance: the contact on the cone, which runs through the
  // vertices, crosses those edges just before their ends. Its end is at
  // the vertex (port patch 0692); before, a sliver of the edge was left
  // between it and the edge's end (as a part's own body made it invalid),
  // the area 1.7e-3 more. The result is the one of the exact body.
  const TopoDS_Shape carried = rib_across_a_recess(2e-5);
  CHECK(BRepCheck_Analyzer(carried).IsValid());
  const TopoDS_Shape rounded_carried = round_recess_rim(carried);
  CHECK(!rounded_carried.IsNull());
  if (!rounded_carried.IsNull()) {
    CHECK(faces_of(rounded_carried) == faces_of(rounded));
    CHECK_NEAR_TOL(area_of(rounded_carried), area_of(rounded), 1e-8);
    CHECK_NEAR_TOL(exact_volume_of(rounded_carried), exact_volume_of(rounded), 1e-9);
  }

  // The same with those edges' curves stopped 2e-5 short of the vertices
  // (mitcad#415): the hatching of the contact on the cone finds its end at
  // the vertex only on the cone's edge with the slope, and the corner took
  // the slope for the face at the end (the cut ran across the bottom, the
  // result invalid). The end is on the edge from the spine's end vertex,
  // the rib's rounding's (port patch 0741): the exact body's result.
  const TopoDS_Shape stopped = rib_across_a_recess(-2e-5);
  CHECK(BRepCheck_Analyzer(stopped).IsValid());
  const TopoDS_Shape rounded_stopped = round_recess_rim(stopped);
  CHECK(!rounded_stopped.IsNull());
  if (!rounded_stopped.IsNull()) {
    CHECK(faces_of(rounded_stopped) == faces_of(rounded));
    CHECK_NEAR_TOL(area_of(rounded_stopped), area_of(rounded), 1e-8);
    CHECK_NEAR_TOL(exact_volume_of(rounded_stopped), exact_volume_of(rounded), 1e-9);
  }
}

// A ring (radii 15 and 23) of two layers, z 0 to 13.5 and 13.5 to 20,
// fused, with a bar (|x| <= 1.5, z 5 to 13.5) across its bore; the upper
// layer notched over the bar up to z = 14.8 and the notch filled again by
// another ring's piece. `split`: the bore's wall kept in its pieces (round
// each end of the bar's top edges three: beside the bar below its top,
// beside it above, and the filler over the bar), copied, so that each
// piece has its own surface and each edge curves on its own faces only;
// else the wall merged into one face.
TopoDS_Shape bar_across_a_split_bore(bool split) {
  const auto ring = [](double z0, double z1) {
    const gp_Ax2 axis(gp_Pnt(0, 0, z0), gp_Dir(0, 0, 1));
    return BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(axis, 23, z1 - z0).Shape(),
                           BRepPrimAPI_MakeCylinder(axis, 15, z1 - z0).Shape())
        .Shape();
  };
  const TopoDS_Shape notch = BRepPrimAPI_MakeBox(gp_Pnt(-1.5, -30, 13.5), gp_Pnt(1.5, 30, 14.8)).Shape();
  const TopoDS_Shape bar =
      BRepAlgoAPI_Common(BRepPrimAPI_MakeBox(gp_Pnt(-1.5, -30, 5), gp_Pnt(1.5, 30, 13.5)).Shape(),
                         BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 15, 20).Shape())
          .Shape();
  const TopoDS_Shape fused =
      BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(ring(0, 13.5), bar).Shape(),
                                        BRepAlgoAPI_Cut(ring(13.5, 20), notch).Shape())
                           .Shape(),
                       BRepAlgoAPI_Common(ring(12, 22), notch).Shape())
          .Shape();
  ShapeUpgrade_UnifySameDomain unify(fused, true, true, true);
  if (split) {
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of_edge;
    TopExp::MapShapesAndUniqueAncestors(fused, TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
    for (int i = 1; i <= faces_of_edge.Extent(); ++i) {
      bool on_bore = faces_of_edge(i).Extent() == 2;
      for (const TopoDS_Shape& face : faces_of_edge(i)) {
        const BRepAdaptor_Surface surface(TopoDS::Face(face));
        on_bore = on_bore && surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - 15) < 1e-7;
      }
      if (on_bore) {
        unify.KeepShape(faces_of_edge.FindKey(i));
      }
    }
  }
  unify.Build();
  return split ? BRepBuilderAPI_Copy(unify.Shape()).Shape() : unify.Shape();
}

int faces_on_bore(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(e.Current()));
    n += surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - 15) < 1e-7 ? 1 : 0;
  }
  return n;
}

void test_bar_top_rounded_into_a_bore_split_round_the_corner() {
  // The bar's top edges rounded with 1, each end on the bore, whose wall
  // round the corner's vertex is three faces of one cylinder. The cut goes
  // on the piece beside the bar below its top, the contact's arc on the
  // filler from the contact to the vertex becoming the edge between those
  // two; that arc has no curve on the piece's own surface (port patch 0740
  // computes it; before, the result was invalid). The pieces stay apart;
  // the volume is the one with the wall in one face.
  const std::vector<gp_Pnt> edges{gp_Pnt(1.5, 0, 13.5), gp_Pnt(-1.5, 0, 13.5)};
  const TopoDS_Shape body = bar_across_a_split_bore(true);
  const TopoDS_Shape merged = bar_across_a_split_bore(false);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  CHECK(faces_on_bore(body) > faces_on_bore(merged));
  const TopoDS_Shape rounded = occt_fillet(body, edges, 1.0);
  const TopoDS_Shape reference = occt_fillet(merged, edges, 1.0);
  CHECK(!rounded.IsNull());
  CHECK(!reference.IsNull());
  if (!rounded.IsNull() && !reference.IsNull()) {
    CHECK(faces_on_bore(rounded) == faces_on_bore(body));
    CHECK(faces_of(rounded) == faces_of(body) + 2);
    CHECK_NEAR_TOL(exact_volume_of(body) - exact_volume_of(rounded), exact_volume_of(merged) - exact_volume_of(reference),
                   1e-7);
  }
}

// A box's vertex given a tolerance of 400 mm (what a rounding left at a
// corner of a part, mitcad#235) counts as inflated: its edges need less
// than a micrometre. Tightened in place, it is what they need, and the box
// stays valid; tolerances below 1 mm, or within ten times the need, are
// left alone.
// The area of the spherical triangle of radius r whose corners lie along
// the unit vectors a, b and c from its centre.
double spherical_triangle_area(const gp_Vec& a, const gp_Vec& b, const gp_Vec& c, double r) {
  const double excess =
      2.0 * std::atan2(std::abs(a.Dot(b.Crossed(c))), 1.0 + a.Dot(b) + b.Dot(c) + c.Dot(a));
  return r * r * excess;
}

void test_bowl_topped_boss_rounded_past_corners_of_the_balls_radius() {
  // A boss 0.8 x 0.8 on a plate, its vertical edges rounded with 0.2, its
  // top cut by the cylinder of radius 1.6 about the line x = 5, z = 3.5
  // along y (a steep bowl: the top climbs from z 1.91 to 2.25 across the
  // boss); then the top's outline rounded with 0.2 (port patch 0670). The
  // walls' roundings are analytic; round each corner the ball's centre
  // stays on the corner's axis, 1.8 from the bowl's, its contact on the
  // top at one point: the rounding there is the spherical triangle between
  // that point and the walls' directions. Before, the walk between the
  // walls' roundings failed at the steep corners.
  const double r = 0.2;
  const double x0 = 5.2, x1 = 6.0, y0 = 3.8, y1 = 4.6, h = kPi / 2.0;
  const auto arc = [&](double cx, double cy, double a0, double a1) {
    return BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(cx, cy, 0.5), gp_Dir(0, 0, 1)), r), a0, a1).Edge();
  };
  BRepBuilderAPI_MakeWire outline;
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0 + r, y0, 0.5), gp_Pnt(x1 - r, y0, 0.5)).Edge());
  outline.Add(arc(x1 - r, y0 + r, 3 * h, 4 * h));
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x1, y0 + r, 0.5), gp_Pnt(x1, y1 - r, 0.5)).Edge());
  outline.Add(arc(x1 - r, y1 - r, 0, h));
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x1 - r, y1, 0.5), gp_Pnt(x0 + r, y1, 0.5)).Edge());
  outline.Add(arc(x0 + r, y1 - r, h, 2 * h));
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, y1 - r, 0.5), gp_Pnt(x0, y0 + r, 0.5)).Edge());
  outline.Add(arc(x0 + r, y0 + r, 2 * h, 3 * h));
  const TopoDS_Shape boss =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire(), true).Face(), gp_Vec(0, 0, 2.5)).Shape();
  BRepAlgoAPI_Fuse fuse(BRepPrimAPI_MakeBox(gp_Pnt(4, 3, 0), gp_Pnt(7, 5.4, 0.5)).Shape(), boss);
  fuse.SimplifyResult();
  BRepAlgoAPI_Cut cut(fuse.Shape(), BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5, 2, 3.5), gp_Dir(0, 1, 0)), 1.6, 4).Shape());
  cut.SimplifyResult();
  const TopoDS_Shape body = cut.Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // The top's outline: the edges of the bowl's face.
  BRepFilletAPI_MakeFillet maker(body);
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface top(TopoDS::Face(f.Current()));
    if (top.GetType() != GeomAbs_Cylinder || std::abs(top.Cylinder().Radius() - 1.6) > 1e-9) {
      continue;
    }
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      if (maker.Contour(TopoDS::Edge(e.Current())) == 0) {
        maker.Add(r, TopoDS::Edge(e.Current()));
      }
    }
  }
  maker.Build();
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  const TopoDS_Shape rounded = maker.Shape();
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  CHECK(volume_of(rounded) < volume_of(body));
  for (const double cx : {x0 + r, x1 - r}) {
    for (const double cy : {y0 + r, y1 - r}) {
      // The ball's centre, its contact on the top towards the bowl's axis,
      // and the walls' directions at this corner.
      const double zc = 3.5 - std::sqrt(1.8 * 1.8 - (cx - 5.0) * (cx - 5.0));
      const gp_Vec toTop = gp_Vec(5.0 - cx, 0.0, 3.5 - zc).Normalized();
      const gp_Vec alongX(cx < 5.6 ? -1.0 : 1.0, 0.0, 0.0), alongY(0.0, cy < 4.2 ? -1.0 : 1.0, 0.0);
      double area = 0.0;
      CHECK(faces_on_sphere(rounded, gp_Pnt(cx, cy, zc), r, 1e-6, area) == 1);
      CHECK_NEAR_TOL(area, spherical_triangle_area(toTop, alongX, alongY, r), 2e-5);
    }
  }
}

void test_inflated_vertex_tolerances_tightened() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(10, 20, 30)).Shape();
  TopExp_Explorer first(box, TopAbs_VERTEX);
  const TopoDS_Vertex vertex = TopoDS::Vertex(first.Current());
  BRep_Builder().UpdateVertex(vertex, 400.0);
  CHECK(mitcad::geometry::detail::inflated_vertices(box, false) == 1);
  CHECK(BRep_Tool::Tolerance(vertex) == 400.0);
  CHECK(mitcad::geometry::detail::inflated_vertices(box, true) == 1);
  CHECK(BRep_Tool::Tolerance(vertex) < 1e-6);
  CHECK(mitcad::geometry::detail::inflated_vertices(box, false) == 0);
  CHECK(BRepCheck_Analyzer(box).IsValid());
  BRep_Builder().UpdateVertex(vertex, 0.5);
  CHECK(mitcad::geometry::detail::inflated_vertices(box, false) == 0);
}

} // namespace

// Each test is guarded by its name, so that MITCAD_TEST_ONLY runs a port
// patch's tests alone at the patch's place in the chain, without the cases
// of later patches.
void fillet_result_tests() {
  guarded("test_slot_end_beside_a_split_side", test_slot_end_beside_a_split_side);
  guarded("test_rounding_ends_on_a_pads_split_foot", test_rounding_ends_on_a_pads_split_foot);
  guarded("test_closed_rounding_rolling_from_a_seam_vertex", test_closed_rounding_rolling_from_a_seam_vertex);
  guarded("test_plate_edges_rounded_to_a_full_round", test_plate_edges_rounded_to_a_full_round);
  guarded("test_top_rounded_past_a_corner_of_the_balls_radius", test_top_rounded_past_a_corner_of_the_balls_radius);
  guarded("test_closed_top_rounded_past_four_corners", test_closed_top_rounded_past_four_corners);
  guarded("test_pocket_floor_rounded_past_its_corners", test_pocket_floor_rounded_past_its_corners);
  guarded("test_rim_rounded_across_a_notch", test_rim_rounded_across_a_notch);
  guarded("test_rounding_carried_across_a_notch", test_rounding_carried_across_a_notch);
  guarded("test_closed_rim_rounded_across_a_notch", test_closed_rim_rounded_across_a_notch);
  guarded("test_extension_rolling_on_past_a_seam", test_extension_rolling_on_past_a_seam);
  guarded("test_rib_rounded_to_a_full_round_out_of_a_plate", test_rib_rounded_to_a_full_round_out_of_a_plate);
  guarded("test_rib_rounded_to_a_full_round_out_of_a_ring", test_rib_rounded_to_a_full_round_out_of_a_ring);
  guarded("test_slot_rounded_to_a_full_round_into_a_floor", test_slot_rounded_to_a_full_round_into_a_floor);
  guarded("test_roundings_cut_by_each_other_where_the_top_narrows",
          test_roundings_cut_by_each_other_where_the_top_narrows);
  guarded("test_concave_roundings_cut_by_each_other_where_a_channel_narrows",
          test_concave_roundings_cut_by_each_other_where_a_channel_narrows);
  guarded("test_rib_foot_rounded_across_the_rods_seam", test_rib_foot_rounded_across_the_rods_seam);
  guarded("test_bowl_topped_boss_rounded_past_corners_of_the_balls_radius",
          test_bowl_topped_boss_rounded_past_corners_of_the_balls_radius);
  guarded("test_bar_top_rounded_into_a_bore_along_its_seam", test_bar_top_rounded_into_a_bore_along_its_seam);
  guarded("test_recess_rim_rounded_up_to_a_rib_across_it", test_recess_rim_rounded_up_to_a_rib_across_it);
  guarded("test_inflated_vertex_tolerances_tightened", test_inflated_vertex_tolerances_tightened);
  guarded("test_bar_top_rounded_into_a_bore_split_round_the_corner",
          test_bar_top_rounded_into_a_bore_split_round_the_corner);
}

} // namespace test
