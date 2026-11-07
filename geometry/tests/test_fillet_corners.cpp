// SPDX-License-Identifier: MIT
// Corners where a rounding ends, built here with OCCT's fillet alone on
// synthetic shapes: the cases behind the patches 0130 and later of the OCCT
// port (third_party/vcpkg-ports, mitcad#236).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <functional>
#include <initializer_list>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <Bnd_Box.hxx>
#include <GeomConvert.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <gp_Trsf.hxx>

#include <BRepAdaptor_Curve.hxx>
#include <NCollection_Map.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepLib.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Precision.hxx>
#include <ShapeFix_ShapeTolerance.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Vec2d.hxx>
#include <BRepTools.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9 (OCCT's default
// integration errs by more than the differences checked here on the
// roundings' spline faces).
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    ++n;
  }
  return n;
}

// Builds the operation; false when OCCT throws or does not finish.
bool build(BRepFilletAPI_MakeFillet& op) {
  try {
    OCC_CATCH_SIGNALS
    op.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet corner test: %s\n", failure.what());
    return false;
  }
  return op.IsDone();
}

// A block 40 x 20 x 25 whose four vertical edges are rounded with a radius
// of 2, cut to a vault: below z = 10 the block, above it a cylinder of
// radius r along x whose axis lies at y = 10, z = 10. With r = 10 the vault
// springs tangentially from the walls y = 0 and y = 20; a larger r makes it
// cross them at an angle of sqrt(2 (r - 10) / 10).
TopoDS_Shape vaulted_block(double r) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(40, 20, 25)).Shape();
  BRepFilletAPI_MakeFillet corners(block);
  for (TopExp_Explorer e(block, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9) {
      corners.Add(2.0, edge);
    }
  }
  corners.Build();
  const TopoDS_Shape lower = BRepPrimAPI_MakeBox(gp_Pnt(-1, -1, -1), gp_Pnt(41, 21, 10)).Shape();
  const TopoDS_Shape vault =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1, 10, 10), gp_Dir(1, 0, 0)), r, 42).Shape();
  const TopoDS_Shape tool = BRepAlgoAPI_Fuse(lower, vault).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Common(corners.Shape(), tool).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The edges of the end x = 0 above the vault's springing: the arc on the end
// face and the two curves where the vault meets the corner roundings.
void add_vault_end(BRepFilletAPI_MakeFillet& fillet, const TopoDS_Shape& body, double radius) {
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    const bool vertical = std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9;
    if (!vertical && m.X() < 2 + 1e-6 && m.Z() > 10 + 1e-6 && a.Z() > 10 - 1e-6 && b.Z() > 10 - 1e-6 &&
        fillet.Contour(edge) == 0) {
      fillet.Add(radius, edge);
    }
  }
}

// The vault's end rounded with a radius of 1 (patch 0130): the curves where
// the vault meets the corner roundings end where both come tangent to the
// wall, and the rounding vanishes there. With a vault that misses tangency by
// a micrometre (radius 10.000001, faces 0.00045 rad apart at the vertex)
// OCCT stopped with an end section of micrometres off the vertex and failed
// at the corner; with radius 10.00001 it built a cap of micrometres whose
// wire intersects itself. Now both end in a point at the vertex, as the
// tangent vault (radius 10) does, and remove the same volume.
void test_rounding_vanishing_where_its_faces_come_tangent() {
  double tangent_removed = 0.0;
  for (const double r : {10.0, 10.000001, 10.00001}) {
    const TopoDS_Shape body = vaulted_block(r);
    BRepFilletAPI_MakeFillet fillet(body);
    add_vault_end(fillet, body, 1.0);
    CHECK(fillet.NbContours() == 1);
    CHECK(build(fillet));
    if (!fillet.IsDone()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
    CHECK(face_count(fillet.Shape()) == face_count(body) + 5);
    const double removed = volume_of(body) - volume_of(fillet.Shape());
    if (r == 10.0) {
      tangent_removed = removed;
      CHECK(removed > 4.8 && removed < 4.9);
    } else {
      CHECK_NEAR_TOL(removed, tangent_removed, 1e-4);
    }
  }
}

// A prism 20 long along x whose end x = 0 is an octagon: the square
// y, z in [0, 10] with its corners cut at 45 degrees, legs of 2 (the top
// edge 6 long, from y = 2 to y = 8).
TopoDS_Shape octagonal_prism() {
  BRepBuilderAPI_MakePolygon outline;
  const std::pair<double, double> corners[] = {{2, 0}, {8, 0}, {10, 2}, {10, 8}, {8, 10}, {2, 10}, {0, 8}, {0, 2}};
  for (const auto& yz : corners) {
    outline.Add(gp_Pnt(0, yz.first, yz.second));
  }
  outline.Close();
  const TopoDS_Face end = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(end, gp_Vec(20, 0, 0)).Shape();
}

// The edge of shape between points a and b.
TopoDS_Edge edge_between(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b) {
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt p = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt q = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if ((p.Distance(a) < 1e-9 && q.Distance(b) < 1e-9) || (p.Distance(b) < 1e-9 && q.Distance(a) < 1e-9)) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// What a rounding of radius r removes along a straight edge from a to b
// between two perpendicular faces whose inward normals are n1 and n2: the
// square r x r beside the edge less the ball's quarter disc, swept along
// the edge 30 long both ways (cut by the body, it ends where the faces at
// its ends cut it).
TopoDS_Shape rounded_away(const gp_Pnt& a, const gp_Pnt& b, const gp_Dir& n1, const gp_Dir& n2, double r) {
  gp_Dir along(gp_Vec(a, b));
  if (along.Crossed(n1).Dot(n2) < 0) {
    along.Reverse();
  }
  const gp_Pnt start = gp_Pnt(0.5 * (a.XYZ() + b.XYZ())).Translated(-15.0 * gp_Vec(along));
  const TopoDS_Shape square = BRepPrimAPI_MakeBox(gp_Ax2(start, along, n1), r, r, 30).Shape();
  const gp_Pnt axis = start.Translated(r * gp_Vec(n1) + r * gp_Vec(n2));
  const TopoDS_Shape ball = BRepPrimAPI_MakeCylinder(gp_Ax2(axis, along), r, 30).Shape();
  return BRepAlgoAPI_Cut(square, ball).Shape();
}

// The two upper corners of the octagonal end rounded with a radius of 3,
// each a rounding of its own (patch 0210): on the top edge each rounding's
// setback is 3 sqrt(2) = 4.24, more than half its 6, so the two roundings'
// cuts by the top face crossed and OCCT failed ("fillets have too big
// radiuses"). Now the two blends are mitred: each is trimmed by the other
// and by the top face, the top edge vanishes, and the body is the octagonal
// prism less what each rounding removes along its edge.
void test_two_roundings_mitred_at_a_short_edge() {
  const TopoDS_Shape body = octagonal_prism();
  BRepFilletAPI_MakeFillet fillet(body);
  const gp_Pnt left_low(0, 0, 8), left_high(0, 2, 10), right_high(0, 8, 10), right_low(0, 10, 8);
  const TopoDS_Edge left = edge_between(body, left_low, left_high);
  const TopoDS_Edge right = edge_between(body, right_high, right_low);
  CHECK(!left.IsNull() && !right.IsNull());
  if (left.IsNull() || right.IsNull()) {
    return;
  }
  fillet.Add(3.0, left);
  fillet.Add(3.0, right);
  CHECK(fillet.NbContours() == 2);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  CHECK(face_count(fillet.Shape()) == face_count(body) + 2);
  CHECK(edge_between(fillet.Shape(), left_high, right_high).IsNull());
  const double s = std::sqrt(0.5);
  // (The inward normals of the end face and of the two upper bevels.)
  const gp_Dir into(1, 0, 0), below_left(0, s, -s), below_right(0, -s, -s);
  TopoDS_Shape expected = BRepAlgoAPI_Cut(body, rounded_away(left_low, left_high, into, below_left, 3.0)).Shape();
  expected = BRepAlgoAPI_Cut(expected, rounded_away(right_high, right_low, into, below_right, 3.0)).Shape();
  const double removed = volume_of(body) - volume_of(fillet.Shape());
  CHECK(removed > 1.0);
  // (Within the faces' boundaries' approximation: a rounding of one of the
  // edges alone, with no mitre, differs from its swept square by 1.7e-5.)
  CHECK_NEAR_TOL(removed, volume_of(body) - volume_of(expected), 5e-5);
}

// A cylinder of radius 10 along z whose seam runs at the angle a from x,
// cut by a gable roof: two planes of slope 0.5 meeting in a ridge along x
// at z = 20, which ends on the cylinder at (10, 0, 20) and (-10, 0, 20).
TopoDS_Shape gabled_cylinder(double a) {
  const TopoDS_Shape cylinder =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(std::cos(a), std::sin(a), 0)), 10, 30)
      .Shape();
  BRepBuilderAPI_MakePolygon gable;
  for (const auto& yz : {std::pair<double, double>{-20, -1}, {20, -1}, {20, 10}, {0, 20}, {-20, 10}}) {
    gable.Add(gp_Pnt(-20, yz.first, yz.second));
  }
  gable.Close();
  const TopoDS_Shape roof =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(gable.Wire(), true).Face(), gp_Vec(40, 0, 0)).Shape();
  return BRepAlgoAPI_Common(cylinder, roof).Shape();
}

// The gabled cylinder's ridge rounded with a radius of 2; the volume it
// removes, or a negative value when the rounding fails or is invalid.
double ridge_rounding_removes(const TopoDS_Shape& body) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.Y()) < 1e-7 && std::abs(b.Y()) < 1e-7 && a.Z() > 19.9 && b.Z() > 19.9 &&
        BRepAdaptor_Curve(edge).GetType() == GeomAbs_Line) {
      fillet.Add(2.0, edge);
    }
  }
  if (fillet.NbContours() != 1 || !build(fillet) || !BRepCheck_Analyzer(fillet.Shape()).IsValid()) {
    return -1.0;
  }
  return volume_of(body) - volume_of(fillet.Shape());
}

// The ridge's end at (10, 0, 20) is cut by the cylinder, whose seam crosses
// that cut where it runs 0.02 rad from the ridge (patch 0211): the arcs that
// hold the rounding's contacts there have their curves on the cylinder on
// the seam's two sides, a period apart, and the cut was sought round the
// whole cylinder (OCCT built an invalid body without its floor). Now it
// crosses the seam, and the rounding removes what it removes with the seam a
// quarter turn away.
void test_rounding_cut_by_a_cylinder_across_its_seam() {
  const double away = ridge_rounding_removes(gabled_cylinder(2.0 * std::atan(1.0)));
  CHECK(away > 2.8 && away < 3.0);
  CHECK_NEAR_TOL(ridge_rounding_removes(gabled_cylinder(0.02)), away, 1e-6);
}

// A ring of outer radius 15.3612 (z in [-2, 0]) whose top edge is rounded
// with 1.2 (its top plane turns tangentially into the rim's torus at radius
// 14.1612) on a core of radius 13 (z in [-5, -2]), and a pillar of radius 1
// whose axis lies at 14.361 from the ring's (z in [-5, 0]), fused, the tops
// one plane: the pillar's wall stands above the rim's torus, highest at its
// outer side. Its seam lies at the angle a from x.
TopoDS_Shape pillar_on_rimmed_ring(double a) {
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -5), gp_Dir(0, 0, 1)), 15.3612, 5).Shape();
  BRepFilletAPI_MakeFillet rim(disc);
  for (TopExp_Explorer e(disc, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Pnt(TopExp::FirstVertex(edge)).Z() > -1e-9 && BRepAdaptor_Curve(edge).GetType() == GeomAbs_Circle) {
      rim.Add(1.2, edge);
    }
  }
  rim.Build();
  const TopoDS_Shape below = BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, -6), gp_Pnt(30, 30, -2)).Shape(),
                                             BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -7), gp_Dir(0, 0, 1)), 13, 6).Shape())
                               .Shape();
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(rim.Shape(), below).Shape();
  const TopoDS_Shape pillar =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(14.361, 0, -5), gp_Dir(0, 0, 1), gp_Dir(std::cos(a), std::sin(a), 0)), 1, 5)
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(ring, pillar).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The pillar's top edge rounded with 0.79, the result, or a null shape when
// the rounding fails.
TopoDS_Shape round_pillar_top(const TopoDS_Shape& body) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - 1) < 1e-7 &&
        curve.Circle().Location().Distance(gp_Pnt(14.361, 0, 0)) < 1e-7) {
      fillet.Add(0.79, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  if (fillet.NbContours() != 1 || !build(fillet)) {
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// Whether the shape has a vertex within tol of p.
bool has_vertex_at(const TopoDS_Shape& shape, const gp_Pnt& p, double tol) {
  for (TopExp_Explorer e(shape, TopAbs_VERTEX); e.More(); e.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(e.Current())).Distance(p) < tol) {
      return true;
    }
  }
  return false;
}

// The pillar's top edge rounded with 0.79 (patch 0280): at both ends the
// wall's contact reaches the wall's edge with the rim's torus before the
// edge's vertex on the torus' tangent circle, and the top's contact (a
// circle of radius 0.21 round the pillar's axis) crosses that circle past
// the vertex. The blend is cut by the torus up to that crossing, to which
// the tangent circle runs on. OCCT failed at the corner: the crossing lies
// on the contact circle's prolongation past its parameter origin (rejected
// as out of range), and the circle's missing end was sought as the
// intersection of the top and the torus, which touch along it. Now the
// rounding is one face more, its corners end at the crossings, and it
// removes the same with the pillar's seam turned elsewhere.
void test_rounding_ending_where_the_top_turns_into_a_rim() {
  // (The crossings of the contact circle with the tangent circle.)
  const double d = 14.361, r1 = 0.21, r2 = 14.1612;
  const double x = (d * d + r2 * r2 - r1 * r1) / (2 * d), y = std::sqrt(r2 * r2 - x * x);
  double first = -1.0;
  for (const double a : {3.0, 2.5}) {
    const TopoDS_Shape body = pillar_on_rimmed_ring(a);
    const TopoDS_Shape result = round_pillar_top(body);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    CHECK(face_count(result) == face_count(body) + 1);
    // (The circles cross at an angle of 0.3 rad, and the contact meets the
    // torus tangentially: the crossing found lies a few 1e-4 off.)
    CHECK(has_vertex_at(result, gp_Pnt(x, y, 0), 1e-3));
    CHECK(has_vertex_at(result, gp_Pnt(x, -y, 0), 1e-3));
    const double removed = volume_of(body) - volume_of(result);
    CHECK(removed > 0.2 && removed < 0.3);
    if (first < 0) {
      first = removed;
    } else {
      CHECK_NEAR_TOL(removed, first, 1e-7);
    }
  }
}

// The octant x, y, z in [-10, 0] whose vertical edge x = y = 0 is cut below
// (0, 0, -0.1) by the plane x + y = z + 0.1: the corner (0, 0, 0) keeps a
// short edge 0.1 long down to the cut. A notch (a box 1 wide, 1 high) at a
// depth of 4 on the cut's line on the face x = 0 splits that line in two
// edges, both between that face and the cut; the face x = 0 is built with
// its wire starting at the lower one (an order the booleans may give as
// well).
TopoDS_Shape octant_with_cut_corner() {
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -10), gp_Pnt(0, 0, 0)).Shape();
  const gp_Pln cut(gp_Pnt(0, 0, -0.1), gp_Dir(gp_Vec(1, 1, -1).Normalized()));
  const TopoDS_Face plane = BRepBuilderAPI_MakeFace(cut, -50, 50, -50, 50).Face();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakePrism(plane, gp_Vec(1, 1, -1).Normalized() * 30.0).Shape()).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-0.5, -4.4, -4.5), gp_Pnt(0.5, -3.4, -3.5)).Shape()).Shape();
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane || std::abs(std::abs(surface.Plane().Axis().Direction().X()) - 1) > 1e-9 ||
        !has_vertex_at(face, gp_Pnt(0, 0, -0.1), 1e-7)) {
      continue;
    }
    std::vector<TopoDS_Edge> edges;
    for (BRepTools_WireExplorer w(BRepTools::OuterWire(face), face); w.More(); w.Next()) {
      edges.push_back(w.Current());
    }
    std::size_t start = 0;
    for (std::size_t i = 0; i < edges.size(); ++i) {
      const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edges[i]));
      const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edges[i]));
      if (std::abs(a.Y() - a.Z() - 0.1) < 1e-7 && std::abs(b.Y() - b.Z() - 0.1) < 1e-7 && std::min(a.Z(), b.Z()) < -4.6) {
        start = i;
      }
    }
    BRepBuilderAPI_MakeWire wire;
    for (std::size_t k = 0; k < edges.size(); ++k) {
      wire.Add(edges[(start + k) % edges.size()]);
    }
    TopoDS_Face rebuilt = BRepBuilderAPI_MakeFace(BRep_Tool::Surface(face), wire.Wire(), true).Face();
    if (rebuilt.Orientation() != face.Orientation()) {
      rebuilt.Reverse();
    }
    occ::handle<BRepTools_ReShape> reshape = new BRepTools_ReShape();
    reshape->Replace(face, rebuilt);
    return reshape->Apply(body);
  }
  return body;
}

// What a rounding of radius r of the octant's edge y = z = 0 removes from
// x = -10 up to the face x = 0 and the cut x + y = z + 0.1: the square r x r
// beside the edge less the quarter disc, swept to where the cut or that
// face ends it (integrated on a fine grid).
double rounded_up_to_cut(double r) {
  const int n = 1000;
  const double h = r / n;
  double sum = 0.0;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      const double y = -r + (i + 0.5) * h, z = -r + (j + 0.5) * h;
      if ((y + r) * (y + r) + (z + r) * (z + r) >= r * r) {
        sum += (std::min(0.0, z + 0.1 - y) + 10.0) * h * h;
      }
    }
  }
  return sum;
}

// The octant's edge y = z = 0 rounded with 1 up to the corner (patch 0281):
// the contact on y = 0 meets the cut before the corner, and the walk round
// the corner goes from there through the short edge to the face x = 0; the
// common edge of the cut and that face at the short edge's end is the upper
// of their two edges, OCCT took the first found, the lower one, and failed
// at the corner. Now the rounding removes what it removes up to the cut.
void test_walk_round_the_corner_past_a_short_edge() {
  const TopoDS_Shape body = octant_with_cut_corner();
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.Y()) < 1e-9 && std::abs(b.Y()) < 1e-9 && std::abs(a.Z()) < 1e-9 && std::abs(b.Z()) < 1e-9) {
      fillet.Add(1.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  CHECK(face_count(fillet.Shape()) == face_count(body) + 1);
  CHECK_NEAR_TOL(volume_of(body) - volume_of(fillet.Shape()), rounded_up_to_cut(1.0), 1e-4);
}

// A floor z <= 0 and a wall x <= 0 along y in [-20, 20], built as two
// halves split at y = 7 (their faces merged, the edge between floor and
// wall keeping the split), with a hole 10 deep down from the floor beside
// the wall: its outline from the wall at y = 6 by a 45 degree chamfer to
// its side y = 7, to x = 8, and back the same way to the wall at y = -6.
TopoDS_Shape floor_wall_with_hole() {
  const auto half = [](double y0, double y1) {
    return BRepAlgoAPI_Fuse(BRepPrimAPI_MakeBox(gp_Pnt(-5, y0, -15), gp_Pnt(20, y1, 0)).Shape(),
                            BRepPrimAPI_MakeBox(gp_Pnt(-5, y0, -15), gp_Pnt(0, y1, 10)).Shape())
      .Shape();
  };
  const TopoDS_Shape halves = BRepAlgoAPI_Fuse(half(-20, 7), half(7, 20)).Shape();
  BRepBuilderAPI_MakePolygon outline;
  for (const auto& xy : {std::pair<double, double>{0, 6}, {1, 7}, {8, 7}, {8, -7}, {1, -7}, {0, -6}}) {
    outline.Add(gp_Pnt(xy.first, xy.second, -10));
  }
  outline.Close();
  const TopoDS_Shape hole =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire(), true).Face(), gp_Vec(0, 0, 20)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(halves, hole).Shape(), false, true, true);
  unify.Build();
  return unify.Shape();
}

// The edge between floor and wall from y = 20 to the hole, rounded with 2;
// the result, or a null shape when the rounding fails.
TopoDS_Shape round_wall_foot(const TopoDS_Shape& body) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.X()) < 1e-9 && std::abs(b.X()) < 1e-9 && std::abs(a.Z()) < 1e-9 && std::abs(b.Z()) < 1e-9 &&
        a.Y() > 5.9 && b.Y() > 5.9 && fillet.Contour(edge) == 0) {
      fillet.Add(2.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  if (fillet.NbContours() != 1 || !build(fillet)) {
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// What the rounding of the wall's foot adds: the square 2 x 2 in the corner
// less the quarter disc, from y = 20 down to the hole's side (y = 7, x >= 1)
// and its chamfer (y = 6 + x).
double added_up_to_hole() {
  const double r = 2.0;
  const int n = 1000;
  const double h = r / n;
  double sum = 0.0;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      const double x = (i + 0.5) * h, z = (j + 0.5) * h;
      if ((x - r) * (x - r) + (z - r) * (z - r) >= r * r) {
        sum += (20.0 - (x >= 1.0 ? 7.0 : 6.0 + x)) * h * h;
      }
    }
  }
  return sum;
}

// The wall's foot rounded up to the hole (patch 0282): the floor's contact
// meets the hole's side, the wall's runs on along the wall; the blend is
// cut by the hole's side and chamfer continued up through it, the wall's
// contact ending where the chamfer's plane meets it, above the chamfer's
// face. OCCT sought that end on the chamfer's face only and failed. Now the
// rounding adds what it adds up to the hole.
void test_rounding_cut_by_faces_past_their_bounds() {
  const TopoDS_Shape body = floor_wall_with_hole();
  const TopoDS_Shape result = round_wall_foot(body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK_NEAR_TOL(volume_of(result) - volume_of(body), added_up_to_hole(), 1e-4);
}

// The same rounding is one face (patch 0283): its piece along the edge from
// the split at y = 7 to the hole, whose floor contact stands at the hole's
// side, was a face of its own, cut off at y = 7.
void test_rounding_one_face_up_to_the_hole() {
  const TopoDS_Shape body = floor_wall_with_hole();
  const TopoDS_Shape result = round_wall_foot(body);
  CHECK(!result.IsNull());
  if (!result.IsNull()) {
    CHECK(face_count(result) == face_count(body) + 1);
  }
}

// A ceiling z = 0 over a cavity y < 0 against a wall y = 0 (the solid an L
// in the section, x in [-10, 10]), with a groove along their junction: the
// cavity runs on above the ceiling between the wall and the plane through
// y = -0.3, z = 0 and y = 0, z = 1. A rib x in [-2, 2], y in [-3, 0] stands
// against the wall up to the ceiling; under the groove its top is a face of
// its own, beside the ceiling in the same plane, the two meeting at the
// points where the groove's side crosses the rib's sides.
TopoDS_Shape ceiling_groove_rib() {
  TopoDS_Shape body = BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -10), gp_Pnt(10, 10, 10)).Shape(),
                                      BRepPrimAPI_MakeBox(gp_Pnt(-11, -11, -11), gp_Pnt(11, 0, 0)).Shape())
                        .Shape();
  body = BRepAlgoAPI_Fuse(body, BRepPrimAPI_MakeBox(gp_Pnt(-2, -3, -10), gp_Pnt(2, 0, 0)).Shape()).Shape();
  BRepBuilderAPI_MakePolygon section;
  section.Add(gp_Pnt(-11, -0.3, 0));
  section.Add(gp_Pnt(-11, 0, 0));
  section.Add(gp_Pnt(-11, 0, 1));
  section.Close();
  const TopoDS_Shape groove =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(section.Wire(), true).Face(), gp_Vec(22, 0, 0)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(body, groove).Shape(), false, true, true);
  unify.Build();
  return unify.Shape();
}

// The rib's feet against the wall rounded with 1 up to the ceiling (patch
// 0290): at each foot's top the contact on the rib's side meets the ceiling
// past the point where the ceiling, the groove's side and the rib's top
// meet it; the walk round the corner from there found no edge between the
// ceiling and the rib's top, which meet at that point only. The fillet's
// end passes from the ceiling to the rib's top, grown under the groove,
// through the groove's side at a point. Each foot adds the square 1 x 1
// less the quarter disc over the height of 10.
void test_walk_round_the_corner_through_a_face_met_at_a_point() {
  const TopoDS_Shape body = ceiling_groove_rib();
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(std::abs(a.X()) - 2) < 1e-9 && std::abs(b.X() - a.X()) < 1e-9 && std::abs(a.Y()) < 1e-9 &&
        std::abs(b.Y()) < 1e-9) {
      fillet.Add(1.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 2);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  CHECK(face_count(fillet.Shape()) == face_count(body) + 2);
  CHECK_NEAR_TOL(volume_of(fillet.Shape()) - volume_of(body), 2 * (1 - kPi / 4) * 10, 1e-6);
}

// A point at angle phi round a circle of radius r about the z axis in the
// plane z = 0, the angle from -y towards +x.
gp_Pnt round_axis(double r, double phi) {
  return gp_Pnt(r * std::sin(phi), -r * std::cos(phi), 0);
}

// A block x in [20 sin(-0.5), 12], y in [-30, -5], z in [-10, 0] outside
// a wall: the plane y = -20 for x <= 0, then the cylinder of radius 20
// about the z axis (tangent to it), up to angle 0.36 or, with `turn`, from
// there on the cylinder's tangent plane. Its top is cut by the plane
// through the cylinder's points at angles 0.3 and 0.7 on the top, falling
// away from the wall at a slope of 1/2: the top a strip along the wall
// narrowing to nothing at angle 0.3, the wall beyond it under the cut.
TopoDS_Shape tapering_strip_on_a_wall(bool turn) {
  const double x0   = 20 * std::sin(-0.5);
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(x0, -30, -10), gp_Pnt(12, -5, 0)).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -20), gp_Dir(0, 0, 1)), 20, 40).Shape())
           .Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(x0 - 1, -20, -11), gp_Pnt(0, 0, 1)).Shape()).Shape();
  if (turn) {
    const gp_Pnt p = round_axis(20, 0.36);
    const gp_Dir t(std::cos(0.36), std::sin(0.36), 0);
    body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Ax2(gp_Pnt(p.X(), p.Y(), -11), gp_Dir(0, 0, 1), t), 30, 30, 22).Shape())
             .Shape();
  }
  const gp_Pnt a = round_axis(20, 0.7), b = round_axis(20, 0.3);
  const gp_Vec along(a, b);
  gp_Vec       away(-along.Y(), along.X(), 0);
  if (away.X() * (a.X() + b.X()) + away.Y() * (a.Y() + b.Y()) < 0) {
    away.Reverse();
  }
  gp_Dir normal(along.Crossed(away.Normalized() + gp_Vec(0, 0, -0.5)));
  if (normal.Z() < 0) {
    normal.Reverse();
  }
  const TopoDS_Face cut = BRepBuilderAPI_MakeFace(gp_Pln(a, normal), -80, 80, -80, 80).Face();
  ShapeUpgrade_UnifySameDomain unify(
    BRepAlgoAPI_Cut(body, BRepPrimAPI_MakePrism(cut, gp_Vec(normal) * 60.0).Shape()).Shape(), false, true, true);
  unify.Build();
  return unify.Shape();
}

// The wall's top edge up to angle 0.3 rounded with 1, or a null shape.
TopoDS_Shape round_wall_top(const TopoDS_Shape& body) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&      edge = TopoDS::Edge(e.Current());
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    const bool on_cylinder    = curve.GetType() == GeomAbs_Circle && std::abs(std::hypot(m.X(), m.Y()) - 20) < 1e-9;
    const bool on_plane       = curve.GetType() == GeomAbs_Line && std::abs(m.Y() + 20) < 1e-9;
    if (std::abs(m.Z()) < 1e-9 && m.X() < 6 && (on_cylinder || on_plane)) {
      fillet.Add(1.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  if (fillet.NbContours() != 1 || !build(fillet)) {
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// The strip's edge with the wall rounded up to where the strip ends, the
// wall running on there as its tangent plane (patch 0291): the top's
// contact runs off the strip across the cut before the end, and the piece
// past it, walked on the top's plane, was dropped at the corner, which
// then failed. The rounding runs on as the blend before it, cut by the
// sloped face and by the wall's tangent plane: it removes what the same
// rounding removes from the block whose wall stays the cylinder, less what
// of that lies past the wall's turn.
void test_rounding_past_the_end_of_a_tapering_strip() {
  const TopoDS_Shape body   = tapering_strip_on_a_wall(true);
  const TopoDS_Shape result = round_wall_top(body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const TopoDS_Shape plain         = tapering_strip_on_a_wall(false);
  const TopoDS_Shape plain_rounded = round_wall_top(plain);
  CHECK(!plain_rounded.IsNull());
  if (plain_rounded.IsNull()) {
    return;
  }
  const gp_Pnt       p = round_axis(20, 0.36);
  const gp_Dir       t(std::cos(0.36), std::sin(0.36), 0);
  const TopoDS_Shape past =
    BRepPrimAPI_MakeBox(gp_Ax2(gp_Pnt(p.X(), p.Y(), -11), gp_Dir(0, 0, 1), t), 30, 30, 22).Shape();
  const TopoDS_Shape removed = BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(plain, plain_rounded).Shape(), past).Shape();
  CHECK_NEAR_TOL(volume_of(body) - volume_of(result), volume_of(removed), 1e-6);
}

// A rod of radius 4.5 along x (x in [0, 30]) with a ridge on it from x = 15
// on: a cylinder of radius 2.5 whose axis lies on the rod's top line (y = 0,
// z = 4.5), its end face x = 15 a lens 2.5 high. The rod's seam runs along
// `seam` (a direction across the axis).
TopoDS_Shape rod_with_ridge(const gp_Dir& seam) {
  const TopoDS_Shape rod = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), seam), 4.5, 30).Shape();
  const TopoDS_Shape ridge =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(15, 0, 4.5), gp_Dir(1, 0, 0), gp_Dir(0, 0, -1)), 2.5, 15).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(rod, ridge).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The lens's edge with the rod rounded with 5, or a null shape.
TopoDS_Shape round_ridge_end(const TopoDS_Shape& body) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&      edge = TopoDS::Edge(e.Current());
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (curve.GetType() == GeomAbs_Circle && std::abs(m.X() - 15) < 1e-9 && std::hypot(m.Y(), m.Z()) < 4.5 + 1e-6) {
      fillet.Add(5.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  if (fillet.NbContours() != 1 || !build(fillet)) {
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// The ridge's end rounded with 5, wider than the lens (patch 0340): the
// ball touches the rod and rolls on the lens's edge with the ridge's side,
// which runs from one end of the rounded edge to the other; past each end
// it rolls on until it comes tangent to the ridge's side. OCCT took the
// lens for the face at the end and failed. Now the ridge's side cuts the
// blend there, its edges with the rod prolonged to the rod's contact: the
// lens is gone (as many faces as before), and all that is added lies
// within the ridge's cylinder prolonged past its end, symmetric about its
// middle plane y = 0.
void test_rounding_cut_by_the_face_of_the_edge_it_rolls_on() {
  const TopoDS_Shape body   = rod_with_ridge(gp_Dir(0, 0, -1));
  const TopoDS_Shape result = round_ridge_end(body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(face_count(result) == face_count(body));
  const TopoDS_Shape added = BRepAlgoAPI_Cut(result, body).Shape();
  const double       v     = volume_of(added);
  CHECK(v > 1.0);
  const TopoDS_Shape prolonged =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 4.5), gp_Dir(1, 0, 0)), 2.5, 15).Shape();
  CHECK_NEAR_TOL(volume_of(BRepAlgoAPI_Common(added, prolonged).Shape()), v, 1e-6);
  GProp_GProps props;
  BRepGProp::VolumeProperties(added, props, 1e-9, true);
  CHECK(std::abs(props.CentreOfMass().Y()) < 1e-4);
}

// The same with the rod's seam at 45 degrees on one side above the axis
// (patch 0341): it runs between the lens's tip on that side and the point
// where the ball comes tangent to the ridge's side, and the walk past that
// end stopped on it, short of that point. The blend is walked again across
// the seam; the body is the one with the seam underneath.
void test_rounding_rolled_on_across_a_seam_past_its_end() {
  const TopoDS_Shape body   = rod_with_ridge(gp_Dir(0, 1, 1));
  const TopoDS_Shape result = round_ridge_end(body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(face_count(result) == face_count(body));
  const TopoDS_Shape plain         = rod_with_ridge(gp_Dir(0, 0, -1));
  const TopoDS_Shape plain_rounded = round_ridge_end(plain);
  CHECK(!plain_rounded.IsNull());
  if (plain_rounded.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(volume_of(BRepAlgoAPI_Cut(result, body).Shape()),
                 volume_of(BRepAlgoAPI_Cut(plain_rounded, plain).Shape()), 1e-5);
}

// A tube (outer radius 20, inner 16, z in [0, 25]) with an ear outside it:
// x in [3, 6], from the tube out to y = -25, z in [5, 20], its outer top
// edge chamfered 4.5, so that its top is a strip 0.7 to 1.4 wide along the
// tube.
TopoDS_Shape tube_with_chamfered_ear() {
  const TopoDS_Shape tube =
    BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 20, 25).Shape(),
                    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 16, 27).Shape())
      .Shape();
  const TopoDS_Shape ear =
    BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(3, -25, 5), gp_Pnt(6, 0, 20)).Shape(),
                    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 20, 27).Shape())
      .Shape();
  // The chamfer: what lies beyond the plane through (y, z) = (-20.5, 20)
  // and (-25, 15.5), outside y = -20.5.
  const double       h = std::sqrt(0.5);
  const gp_Pnt       p = gp_Pnt(-30, -20.5, 20).Translated(-20. * gp_Vec(0, h, h));
  const TopoDS_Shape beyond = BRepPrimAPI_MakeBox(gp_Ax2(p, gp_Dir(0, -h, h), gp_Dir(1, 0, 0)), 60, 40, 40).Shape();
  const TopoDS_Shape chamfer =
    BRepAlgoAPI_Common(beyond, BRepPrimAPI_MakeBox(gp_Pnt(2, -30, 0), gp_Pnt(7, -20.5, 26)).Shape()).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(BRepAlgoAPI_Fuse(tube, ear).Shape(), chamfer).Shape(), true,
                                     true, true);
  unify.Build();
  return unify.Shape();
}

// The ear's top edge with the tube rounded with 2 (patch 0342): the ball
// touches the tube and rolls on the top's edge with the chamfer, and past
// that edge's ends, where the ear's sides meet the top, the stripe went on
// turning about the ends and rolling on the sides' edges, which the
// corners at the ear's sides could not cut (the result was invalid). The
// blend is walked again rolling on the edge's line up to the spine's ends
// and cut by the ear's sides, grown up to the caps from the edge's ends:
// the top is gone (as many faces as before), and all that is added lies
// between the ear's sides.
void test_rounding_rolled_on_past_its_edge_and_cut_by_the_ends() {
  const TopoDS_Shape body = tube_with_chamfered_ear();
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&      edge = TopoDS::Edge(e.Current());
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (curve.GetType() == GeomAbs_Circle && std::abs(m.Z() - 20) < 1e-9 && m.X() > 3 && m.X() < 6) {
      fillet.Add(2.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape& result = fillet.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(face_count(result) == face_count(body));
  const TopoDS_Shape added = BRepAlgoAPI_Cut(result, body).Shape();
  const double       v     = volume_of(added);
  CHECK(v > 1.0);
  const TopoDS_Shape between = BRepPrimAPI_MakeBox(gp_Pnt(3, -30, 0), gp_Pnt(6, 0, 30)).Shape();
  CHECK_NEAR_TOL(volume_of(BRepAlgoAPI_Common(added, between).Shape()), v, 1e-6);
  CHECK(volume_of(BRepAlgoAPI_Cut(body, result).Shape()) < 1e-9);
}

// A block whose front x = 0 has a valley for y in [0, 10]: its sides
// x = 0.1 s^2 |z| / 10 with s = y + 0.04 |z| (z in [-5, 5]) are tangent to
// the front all along their edges with it (the lines y = -0.04 |z|) and to
// each other at V = (0, 0, 0), where the valley's floor (z = 0, from
// y = 10) ends. Each side is two faces, split at s = 5; the far end is cut
// square at y = 9.8 above a triangle closing the valley. With shift > 0 the
// sides' curves of their edges with the front lie that far inside the sides
// (in their parameter, 10 shift in length), within the edges' tolerance of
// 0.003, as on stored bodies.
TopoDS_Shape valley_block(double shift) {
  const double c = 0.1, k = 0.04, L = 10, H = 5, W = 5, D = 5;
  const double X = c * L * H, Yt = L - k * H;
  BRepBuilderAPI_Sewing sew(1.e-6);
  for (const double zs : {1.0, -1.0}) {
    // u along s from L to 0, v from the floor to z = zs H; degree 2 in u.
    NCollection_Array2<gp_Pnt> poles(1, 3, 1, 2);
    const double               ss[3] = {L, L / 2, 0}, s2[3] = {L * L, 0., 0.};
    for (int i = 0; i < 3; i++) {
      poles(i + 1, 1) = gp_Pnt(0, ss[i], 0);
      poles(i + 1, 2) = gp_Pnt(c * s2[i] * H / L, ss[i] - k * H, zs * H);
    }
    NCollection_Array1<double> knots(1, 2);
    knots(1) = 0.;
    knots(2) = 1.;
    NCollection_Array1<int> mu(1, 2), mv(1, 2);
    mu.Init(3);
    mv.Init(2);
    const occ::handle<Geom_BSplineSurface> s = new Geom_BSplineSurface(poles, knots, knots, mu, mv, 2, 1);
    for (const auto& u : {std::pair<double, double>{0., 0.5}, std::pair<double, double>{0.5, 1.}}) {
      const TopoDS_Face f = BRepBuilderAPI_MakeFace(s, u.first, u.second, 0., 1., Precision::Confusion()).Face();
      sew.Add(zs > 0 ? f.Reversed() : TopoDS_Shape(f));
    }
    // The top (bottom), with the side's edges there.
    BRepBuilderAPI_MakeWire wire;
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-D, Yt, zs * H), gp_Pnt(-D, -W, zs * H)).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-D, -W, zs * H), gp_Pnt(0, -W, zs * H)).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(0, -W, zs * H), gp_Pnt(0, -k * H, zs * H)).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(s->VIso(1.), 1., 0.5).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(s->VIso(1.), 0.5, 0.).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(X, Yt, zs * H), gp_Pnt(-D, Yt, zs * H)).Edge());
    sew.Add(BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, 0, zs * H), gp_Dir(0, 0, 1)), wire.Wire(), true).Face());
  }
  const auto polygon = [](std::initializer_list<gp_Pnt> pts) {
    BRepBuilderAPI_MakePolygon p;
    for (const gp_Pnt& q : pts) {
      p.Add(q);
    }
    p.Close();
    return BRepBuilderAPI_MakeFace(p.Wire(), true).Face();
  };
  sew.Add(polygon({gp_Pnt(0, -W, -H), gp_Pnt(0, -k * H, -H), gp_Pnt(0, 0, 0), gp_Pnt(0, -k * H, H), gp_Pnt(0, -W, H)}));
  sew.Add(polygon({gp_Pnt(-D, -W, -H), gp_Pnt(-D, -W, H), gp_Pnt(-D, Yt, H), gp_Pnt(-D, Yt, -H)}));
  sew.Add(polygon({gp_Pnt(-D, -W, -H), gp_Pnt(0, -W, -H), gp_Pnt(0, -W, H), gp_Pnt(-D, -W, H)}));
  sew.Add(polygon({gp_Pnt(X, Yt, H), gp_Pnt(0, L, 0), gp_Pnt(X, Yt, -H)}));
  sew.Add(polygon({gp_Pnt(X, Yt, H), gp_Pnt(X, Yt, -H), gp_Pnt(-D, Yt, -H), gp_Pnt(-D, Yt, H)}));
  sew.Perform();
  TopoDS_Solid solid = BRepBuilderAPI_MakeSolid(TopoDS::Shell(sew.SewedShape())).Solid();
  BRepLib::OrientClosedSolid(solid);
  if (shift > 0.) {
    // The sides' curves of the edges x = 0 below y = 0 (the front's).
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ef;
    TopExp::MapShapesAndAncestors(solid, TopAbs_EDGE, TopAbs_FACE, ef);
    BRep_Builder builder;
    for (int i = 1; i <= ef.Extent(); ++i) {
      const TopoDS_Edge&      e = TopoDS::Edge(ef.FindKey(i));
      const BRepAdaptor_Curve curve(e);
      const gp_Pnt            m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
      if (std::abs(m.X()) > 1e-9 || m.Y() > 1e-9 || m.Y() < -1. || std::abs(m.Z()) < 1e-9) {
        continue;
      }
      for (const TopoDS_Shape& f : ef(i)) {
        const TopoDS_Face& face = TopoDS::Face(f);
        if (BRepAdaptor_Surface(face).GetType() != GeomAbs_BSplineSurface) {
          continue;
        }
        double                          a = 0., b = 0.;
        const occ::handle<Geom2d_Curve> pc = BRep_Tool::CurveOnSurface(e, face, a, b);
        const occ::handle<Geom2d_Curve> moved =
          occ::down_cast<Geom2d_Curve>(pc->Translated(gp_Vec2d(1. - shift - pc->Value(a).X(), 0.)));
        builder.UpdateEdge(e, moved, face, 0.003);
      }
    }
    ShapeFix_ShapeTolerance().LimitTolerance(solid, 0.003, 0.003, TopAbs_SHAPE);
  }
  return solid;
}

// The valley's floor rounded with 1, ending at V where its sides come
// tangent (patch 0400); the volume the rounding adds, or a negative value
// when it fails or the result is invalid.
double valley_rounding_adds(const TopoDS_Shape& body, bool& point_at_v) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt       a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt       b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.X()) < 1e-9 && std::abs(b.X()) < 1e-9 && std::abs(a.Z()) < 1e-9 && std::abs(b.Z()) < 1e-9 &&
        fillet.Contour(edge) == 0) {
      fillet.Add(1.0, edge);
    }
  }
  if (fillet.NbContours() != 1 || !build(fillet) || !BRepCheck_Analyzer(fillet.Shape()).IsValid()) {
    return -1.0;
  }
  point_at_v = has_vertex_at(fillet.Shape(), gp_Pnt(0, 0, 0), 0.003);
  return volume_of(fillet.Shape()) - volume_of(body);
}

// On the exact valley the walk reaches V and the rounding ends in a point
// there. Where the sides' curves of their edges with the front lie 2
// micrometres inside the sides, as on stored bodies, the walk's last piece
// ended where both contacts reached those edges, 0.002 short of the guide's
// end, within V's tolerance: the end handling found the front beyond, the
// stripe went on with a piece from V whose sections, both contacts on the
// front, had no width, and its approximation threw (the zero angle's minimal
// weights, "Geom_TrimmedCurve::U1 == U2"). Now the stripe ends there, in a
// point at V, and the rounding adds what it adds on the exact valley.
void test_rounding_vanishing_where_its_faces_come_tangent_short_of_the_end() {
  bool         exact_point = false, shifted_point = false;
  const double exact       = valley_rounding_adds(valley_block(0.0), exact_point);
  CHECK(exact > 0.35 && exact < 0.36);
  CHECK(exact_point);
  const double shifted = valley_rounding_adds(valley_block(0.0002), shifted_point);
  CHECK(shifted > 0.0);
  CHECK(shifted_point);
  // (The shifted curves move the sides' boundary by 0.002 at most.)
  CHECK_NEAR_TOL(shifted, exact, 2e-3);
}

// The ridge's end at (10, 0, 20) on the cylinder's seam itself (patch
// 0401): the seam starts at the corner's vertex, and the cut of the
// rounding's end by the cylinder passes it at its highest point, 0.24 below
// the vertex. The cut's and the seam's curves meet there only within the
// cut's approximation (2.4e-6), more than the extrema's confusion, so the
// cut was not split at the seam: OCCT built an invalid body without its
// floor. Now the cut is split where its curve on the cylinder crosses the
// seam's line, and the rounding removes what it removes with the seam a
// quarter turn away.
void test_rounding_cut_by_a_cylinder_whose_seam_starts_at_the_vertex() {
  const double away = ridge_rounding_removes(gabled_cylinder(2.0 * std::atan(1.0)));
  CHECK(away > 2.8 && away < 3.0);
  CHECK_NEAR_TOL(ridge_rounding_removes(gabled_cylinder(0.0)), away, 1e-6);
}

// The vault's end rounded with the radius of the block's corner roundings
// (2, patch 0410): the rounding runs over the corners onto the end face, and
// where the vault springs from the walls its section is the corners' own
// arc, which the rounding runs on into tangentially. OCCT found no face at
// the end holding both of the end's points (one lies at the vertex, the
// other inside the end face's edge with the corner) and failed at the
// corner. Now the cap is that section, the corners are cut along it and the
// curves where the vault met them go: the removed volume is the region the
// ball sweeps away (between the vault, the end face and the torus of the
// ball's centre circle, as far as the body goes), and the corner roundings
// keep their faces below the springing.
void test_rounding_ending_in_the_face_it_runs_into() {
  const TopoDS_Shape body = vaulted_block(10.0);
  BRepFilletAPI_MakeFillet fillet(body);
  add_vault_end(fillet, body, 2.0);
  CHECK(fillet.NbContours() == 1);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape& result = fillet.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The ball's centre runs on the circle of radius 8 about the vault's axis
  // in the plane x = 2: the swept-away region is the vault's outer band of 2
  // up to x = 2 (above the springing), less the torus about that circle,
  // within the body.
  const gp_Ax2       axis(gp_Pnt(0, 10, 10), gp_Dir(1, 0, 0));
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(axis, 10.5, 2).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(axis, 8, 2).Shape();
  const TopoDS_Shape above = BRepPrimAPI_MakeBox(gp_Pnt(-1, -1, 10), gp_Pnt(3, 21, 25)).Shape();
  const TopoDS_Shape band  = BRepAlgoAPI_Common(BRepAlgoAPI_Cut(outer, inner).Shape(), above).Shape();
  const TopoDS_Shape torus =
    BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(2, 10, 10), gp_Dir(1, 0, 0)), 8, 2).Shape();
  const TopoDS_Shape swept   = BRepAlgoAPI_Cut(BRepAlgoAPI_Common(band, body).Shape(), torus).Shape();
  const double       removed = volume_of(body) - volume_of(result);
  CHECK(removed > 1.0);
  CHECK_NEAR_TOL(removed, volume_of(swept), 1e-5);
  CHECK(volume_of(BRepAlgoAPI_Cut(result, body).Shape()) < 1e-9);
  // Both corner roundings keep their faces up to the springing, cut along
  // the quarter circle there.
  int corners = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - 2.0) < 1e-9 &&
        surface.Cylinder().Location().X() < 3.0) {
      ++corners;
      for (TopExp_Explorer v(f.Current(), TopAbs_VERTEX); v.More(); v.Next()) {
        CHECK(BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Z() < 10.0 + 1e-6);
      }
    }
  }
  CHECK(corners == 2);
}

// A wall 3.54 thick (x from 0 to 3.54, 30 long in y, 15 high) with a window
// through it (z from 4 to 11, y below 0). The window's end at y = 0 is a
// strip 0.5 wide at the side x = 0, then a face meeting it at `degrees`
// (negative: turning away from the wall's material) up to the other side;
// with `both`, that face ends in a second strip 0.5 wide, parallel to the
// first, a step of `sliver` towards the window away from it.
TopoDS_Shape walled_window(double degrees, bool both, double sliver) {
  const double       t     = std::tan(degrees * M_PI / 180.0);
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, 0), gp_Pnt(3.54, 10, 15)).Shape();
  BRepBuilderAPI_MakePolygon poly;
  poly.Add(gp_Pnt(-1, -10, 4));
  poly.Add(gp_Pnt(4.54, -10, 4));
  if (both) {
    const double ym = 2.54 * t;
    poly.Add(gp_Pnt(4.54, ym - sliver, 4));
    poly.Add(gp_Pnt(3.04, ym - sliver, 4));
    poly.Add(gp_Pnt(3.04, ym, 4));
  } else {
    poly.Add(gp_Pnt(4.54, 4.04 * t, 4));
  }
  poly.Add(gp_Pnt(0.5, 0, 4));
  poly.Add(gp_Pnt(-1, 0, 4));
  poly.Close();
  const TopoDS_Face  face   = BRepBuilderAPI_MakeFace(poly.Wire()).Face();
  const TopoDS_Shape window = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 7)).Shape();
  return BRepAlgoAPI_Cut(block, window).Shape();
}

// The window's vertical edge at (x, y).
TopoDS_Edge window_edge(const TopoDS_Shape& body, double x, double y) {
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt       a    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt       b    = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (a.Distance(gp_Pnt(x, y, a.Z())) < 1e-9 && b.Distance(gp_Pnt(x, y, b.Z())) < 1e-9 &&
        std::abs(a.Z() - b.Z()) > 1.0) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// The section a rounding of radius 2 takes off where it rolls on the far
// edge of the strip 0.5 wide beside its edge: its ball, 2 from the wall,
// passes through that edge (centre (2, yc)); the corner's region between
// the wall's contact, the edge and the arc.
double rolled_section() {
  const double r = 2.0, w = 0.5;
  const double yc = std::sqrt(r * r - (r - w) * (r - w));
  return 0.5 * (r + w) * yc - 0.5 * r * r * std::acos((r - w) / r);
}

// The window's two vertical edges at its end rounded with 2, the end 3.54
// wide (patches 0430 and 0431): the rounding beside the strip rolls on the
// strip's far edge (its ball through that edge, tangent to the wall), the
// other one keeps its face, and their ends on the window's top and bottom
// stay apart. Before, the first one ran over the strip onto the other's face
// (at 0.35 degrees OCCT took the two for tangent), across the other's
// contact there, and the corners failed ("fillets have too big radiuses").
// With strips on both sides both roll, also past a sliver of 0.003 on one
// side. The sections are exact, so are the volumes.
void test_roundings_roll_before_each_others_faces() {
  for (const double degrees : {-0.35, 0.35, -10.0}) {
    const TopoDS_Shape body = walled_window(degrees, false, 0.0);
    BRepFilletAPI_MakeFillet fillet(body);
    const TopoDS_Edge narrow = window_edge(body, 0.0, 0.0);
    const TopoDS_Edge wide   = window_edge(body, 3.54, 3.04 * std::tan(degrees * M_PI / 180.0));
    CHECK(!narrow.IsNull() && !wide.IsNull());
    if (narrow.IsNull() || wide.IsNull()) {
      return;
    }
    fillet.Add(2.0, narrow);
    fillet.Add(2.0, wide);
    CHECK(build(fillet));
    if (!fillet.IsDone()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
    // The wide side's corner: the solid's angle there is 90 degrees plus the
    // turn.
    const double alpha  = M_PI / 2.0 + degrees * M_PI / 180.0;
    const double corner = 4.0 * (1.0 / std::tan(alpha / 2.0) - (M_PI - alpha) / 2.0);
    CHECK_NEAR_TOL(volume_of(body) - volume_of(fillet.Shape()), 7.0 * (rolled_section() + corner), 1e-6);
  }
  for (const double sliver : {0.0, 0.003}) {
    const TopoDS_Shape body = walled_window(-0.35, true, sliver);
    const double       ym   = 2.54 * std::tan(-0.35 * M_PI / 180.0) - sliver;
    BRepFilletAPI_MakeFillet fillet(body);
    const TopoDS_Edge a = window_edge(body, 0.0, 0.0);
    const TopoDS_Edge b = window_edge(body, 3.54, ym);
    CHECK(!a.IsNull() && !b.IsNull());
    if (a.IsNull() || b.IsNull()) {
      return;
    }
    fillet.Add(2.0, a);
    fillet.Add(2.0, b);
    CHECK(build(fillet));
    if (!fillet.IsDone()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
    CHECK_NEAR_TOL(volume_of(body) - volume_of(fillet.Shape()), 2.0 * 7.0 * rolled_section(), 1e-6);
  }
}

// The inner outline (at height z) of a frame 100 square: its sides `off`
// in from the frame's, its corners arcs of radius `rad` about the points 15
// in from two sides, bulging inwards.
TopoDS_Wire frame_outline(double off, double rad, double z) {
  const double S = 100.0, c = 15.0;
  const double e = c + std::sqrt(rad * rad - (c - off) * (c - off)); // where an arc meets a side
  const gp_Pnt pts[8] = {gp_Pnt(off, e, z),         gp_Pnt(e, off, z),         gp_Pnt(S - e, off, z),
                         gp_Pnt(S - off, e, z),     gp_Pnt(S - off, S - e, z), gp_Pnt(S - e, S - off, z),
                         gp_Pnt(e, S - off, z),     gp_Pnt(off, S - e, z)};
  const double q      = rad / std::sqrt(2.0);
  const gp_Pnt mid[4] = {gp_Pnt(c + q, c + q, z), gp_Pnt(S - c - q, c + q, z), gp_Pnt(S - c - q, S - c - q, z),
                         gp_Pnt(c + q, S - c - q, z)};
  BRepBuilderAPI_MakeWire wire;
  for (int k = 0; k < 4; k++) {
    wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(pts[2 * k], mid[k], pts[2 * k + 1]).Value()).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(pts[2 * k + 1], pts[(2 * k + 2) % 8]).Edge());
  }
  return wire.Wire();
}

// A frame 120 square (from -10) between heights z0 and z1 round the hole of
// frame_outline(off, rad).
TopoDS_Shape frame_part(double off, double rad, double z0, double z1) {
  const TopoDS_Shape outer = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, z0), gp_Pnt(110, 110, z1)).Shape();
  const TopoDS_Face  face  = BRepBuilderAPI_MakeFace(frame_outline(off, rad, z0)).Face();
  return BRepAlgoAPI_Cut(outer, BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, z1 - z0)).Shape()).Shape();
}

// A box's rim with a lip: above z = 98 the lip, its inner sides 6 in from the
// frame's and its corners arcs of 15; under it a ring 1 wide (z = 98, in 5
// with corner arcs of 14) and a chamfer of 45 degrees down to the wall below
// z = 97 (in 4, arcs of 13).
TopoDS_Shape lipped_rim() {
  const TopoDS_Shape lip  = frame_part(6.0, 15.0, 98.0, 103.0);
  const TopoDS_Face  face = BRepBuilderAPI_MakeFace(frame_outline(4.0, 13.0, 97.0)).Face();
  const TopoDS_Shape hole = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 1)).Shape();
  BRepOffsetAPI_DraftAngle draft(hole);
  for (TopExp_Explorer f(hole, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(f.Current()));
    if (s.GetType() != GeomAbs_Plane || std::abs(s.Plane().Axis().Direction().Z()) < 0.5) {
      draft.Add(TopoDS::Face(f.Current()), gp_Dir(0, 0, 1), M_PI / 4.0, gp_Pln(gp_Pnt(0, 0, 97), gp_Dir(0, 0, 1)));
    }
  }
  draft.Build();
  const TopoDS_Shape slab =
    BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, 97), gp_Pnt(110, 110, 98)).Shape(), draft.Shape()).Shape();
  const TopoDS_Shape wall = BRepAlgoAPI_Fuse(frame_part(4.0, 13.0, 90.0, 97.0), slab).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(lip, wall).Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

// The lip's inner bottom edges (four lines and four arcs meeting at angles)
// rounded with 4 over the ring 1 wide (patch 0432): the roundings would run
// over the ring onto the chamfer beyond, each onto another face (a plane, a
// cone), and leave their corners no face in common; OCCT failed there
// ("Failed to find edge"). They roll on the ring's far edges instead, the
// arcs' blends the exact tori of balls rolling on circles, and meet in a
// crease from the lip's vertical edge down to the ring's corner. The region
// taken off is the corner regions swept along the edges (prolonged past
// their ends), within the body, less what any ball covers.
void test_roundings_crease_over_a_narrow_ring() {
  const TopoDS_Shape body = lipped_rim();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  BRepFilletAPI_MakeFillet fillet(body);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); i++) {
    const TopoDS_Edge&      edge = TopoDS::Edge(edges(i));
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(m.Z() - 98.0) > 1e-9) {
      continue;
    }
    bool onLip = std::abs(std::min(std::min(m.X(), 100 - m.X()), std::min(m.Y(), 100 - m.Y())) - 6.0) < 1e-6;
    for (const gp_Pnt centre : {gp_Pnt(15, 15, 98), gp_Pnt(85, 15, 98), gp_Pnt(85, 85, 98), gp_Pnt(15, 85, 98)}) {
      onLip = onLip || (curve.GetType() == GeomAbs_Circle && std::abs(m.Distance(centre) - 15.0) < 1e-6);
    }
    if (onLip) {
      fillet.Add(4.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 8);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  // The section (s across the edge: the lip at 6, the ring's far edge at 5;
  // z), the ball's centre at s = 2, z = 98 + sqrt 7.
  const double zc      = 98.0 + std::sqrt(7.0);
  const double a       = 0.5 * std::atan2(98.0 - zc, 3.0);
  const auto   section = [&](const std::function<gp_Pnt(double, double)>& at) {
    BRepBuilderAPI_MakeWire wire;
    wire.Add(BRepBuilderAPI_MakeEdge(at(5, 98), at(6, 98)).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(at(6, 98), at(6, zc)).Edge());
    wire.Add(BRepBuilderAPI_MakeEdge(
               GC_MakeArcOfCircle(at(6, zc), at(2 + 4 * std::cos(a), zc + 4 * std::sin(a)), at(5, 98)).Value())
               .Edge());
    return BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  };
  NCollection_List<TopoDS_Shape> regions, balls;
  const double         margin = 5.0;
  for (int k = 0; k < 4; k++) {
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(gp_Pnt(50, 50, 0), gp_Dir(0, 0, 1)), k * M_PI / 2.0);
    // The left side (x = 6, y from 27 to 73) and the bottom-left corner's arc
    // about (15, 15) (its radius from it s + 9), turned to each side.
    const TopoDS_Face side = section([margin](double s, double z) { return gp_Pnt(s, 27.0 - margin, z); });
    regions.Append(
      BRepBuilderAPI_Transform(BRepPrimAPI_MakePrism(side, gp_Vec(0, 46 + 2 * margin, 0)).Shape(), turn).Shape());
    const double      phi0   = std::atan2(-9.0, 12.0) - margin / 15.0;
    const double      span   = std::atan2(12.0, -9.0) - std::atan2(-9.0, 12.0) + 2 * margin / 15.0;
    const TopoDS_Face corner = section([phi0](double s, double z) {
      return gp_Pnt(15 + (s + 9) * std::cos(phi0), 15 + (s + 9) * std::sin(phi0), z);
    });
    regions.Append(BRepBuilderAPI_Transform(
                     BRepPrimAPI_MakeRevol(corner, gp_Ax1(gp_Pnt(15, 15, 0), gp_Dir(0, 0, 1)), span).Shape(), turn)
                     .Shape());
    balls.Append(BRepBuilderAPI_Transform(
                   BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(2, 27 - margin, zc), gp_Dir(0, 1, 0)), 4.0, 46 + 2 * margin)
                     .Shape(),
                   turn)
                   .Shape());
    balls.Append(BRepBuilderAPI_Transform(
                   BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(15, 15, zc), gp_Dir(0, 0, 1)), 11.0, 4.0).Shape(), turn)
                   .Shape());
  }
  NCollection_List<TopoDS_Shape> bodies;
  bodies.Append(body);
  BRepAlgoAPI_Common within;
  within.SetArguments(bodies);
  within.SetTools(regions);
  within.Build();
  NCollection_List<TopoDS_Shape> taken;
  taken.Append(within.Shape());
  BRepAlgoAPI_Cut uncovered;
  uncovered.SetArguments(taken);
  uncovered.SetTools(balls);
  uncovered.Build();
  const double expected = volume_of(uncovered.Shape());
  CHECK(expected > 250.0);
  CHECK_NEAR_TOL(volume_of(body) - volume_of(fillet.Shape()), expected, 2e-5);
}

// A block `length` long in y whose wall x = 20 stands above a foot sloping
// down outwards at `degrees` (from x = 20, z = h to x = 26, z = 0, h = 6
// tan), the wall's vertical edge at the front y = 0 rounded with 3. The
// rounding's cylinder turns tangentially into the front face.
TopoDS_Shape footed_wall(double degrees, double length, double& h) {
  h = 6.0 * std::tan(degrees * M_PI / 180.0);
  BRepBuilderAPI_MakePolygon poly;
  poly.Add(gp_Pnt(0, 0, 0));
  poly.Add(gp_Pnt(26, 0, 0));
  poly.Add(gp_Pnt(20, 0, h));
  poly.Add(gp_Pnt(20, 0, h + 20));
  poly.Add(gp_Pnt(0, 0, h + 20));
  poly.Close();
  const TopoDS_Shape block =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(poly.Wire()).Face(), gp_Vec(0, length, 0)).Shape();
  BRepFilletAPI_MakeFillet corner(block);
  for (TopExp_Explorer e(block, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt       a    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt       b    = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (a.Distance(gp_Pnt(20, 0, a.Z())) < 1e-9 && b.Distance(gp_Pnt(20, 0, b.Z())) < 1e-9) {
      corner.Add(3.0, edge);
    }
  }
  corner.Build();
  return corner.Shape();
}

// The wall's foot (its concave edge with the slope and that edge's arc on
// the corner's cylinder) rounded with 2, up to the front face (mitcad#341):
// at the front the rounding's contact on the cylinder runs on tangentially
// into the front face, and its contact on the slope reaches the slope's
// sharp edge with it. Both reach the face at the end, which cuts the
// rounding there. OCCT walked the cylinder's side on along the front face
// and found no corner; the walk to that edge runs past the spine's end,
// where on a long spine the contacts run more than twice as fast as at
// the end, and was cut short of the edge (port patch 0450).
void rounding_ending_where_its_face_turns_tangentially(double deg, double length) {
  double             h    = 0.0;
  const double       r    = 2.0;
  const TopoDS_Shape body = footed_wall(deg, length, h);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  BRepFilletAPI_MakeFillet                               fillet(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> foot;
  const double                                           top = h + 3.0 * std::tan(deg * M_PI / 180.0);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt       a    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt       b    = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::min(a.Z(), b.Z()) > h - 1e-6 && std::max(a.Z(), b.Z()) < top + 1e-6
        && std::min(a.X(), b.X()) > 17.0 - 1e-6 && std::max(a.Y(), b.Y()) > 1e-6) {
      foot.Add(edge);
    }
  }
  CHECK(foot.Extent() == 2);
  // (A rounding of exactly these edges: it ends at the front face.)
  fillet.StopChainsAtTurns(foot);
  for (const TopoDS_Shape& edge : foot) {
    fillet.Add(r, TopoDS::Edge(edge));
  }
  CHECK(fillet.NbContours() == 1);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape& result = fillet.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // Along the straight part the concave rounding fills the section between
  // the wall, the slope (90 + deg degrees apart) and the ball's arc.
  const double phi     = (90.0 + deg) * M_PI / 180.0;
  const double section = r * r * (1.0 / std::tan(phi / 2.0) - (M_PI - phi) / 2.0);
  const auto   added   = [&](double y0, double y1) {
    const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(-1, y0, -1), gp_Pnt(30, y1, h + 30)).Shape();
    return volume_of(BRepAlgoAPI_Common(result, slab).Shape()) - volume_of(BRepAlgoAPI_Common(body, slab).Shape());
  };
  CHECK_NEAR_TOL(added(8.0, length), (length - 8.0) * section, 1e-6);
  // Round the corner it fills more than that section: the cylinder and the
  // slope meet at a sharper angle there.
  CHECK(added(0.0, 8.0) > 8.0 * section);
  CHECK(volume_of(BRepAlgoAPI_Cut(body, result).Shape()) < 1e-9);
  // The front face cuts it: the contact on the cylinder ends on the
  // cylinder's edge with the front face, where the ball touching the
  // cylinder there (its centre 2 in front of it, 5 from the axis) touches
  // the slope; no vertex lies in front of that face.
  const double a      = deg * M_PI / 180.0;
  const double zc     = h + (r + 3.0 * std::sin(a)) / std::cos(a);
  bool         cut    = false;
  bool         behind = true;
  for (TopExp_Explorer v(result, TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
    cut            = cut || p.Distance(gp_Pnt(17, 0, zc)) < 1e-6;
    behind         = behind && p.Y() > -1e-7;
  }
  CHECK(zc > top + r);
  CHECK(cut);
  CHECK(behind);
}

void test_rounding_ending_where_its_face_turns_tangentially_into_the_end_face() {
  rounding_ending_where_its_face_turns_tangentially(70.0, 20.0);
  rounding_ending_where_its_face_turns_tangentially(60.0, 200.0);
}

// The edges of `shape` whose middle point satisfies `keep`.
std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<TopoDS_Edge>                                     out;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(edges(i));
    if (BRep_Tool::Degenerated(e)) {
      continue;
    }
    const BRepAdaptor_Curve c(e);
    if (keep(c.Value(0.5 * (c.FirstParameter() + c.LastParameter())))) {
      out.push_back(e);
    }
  }
  return out;
}

// The fillet of `edges` with radius r; a null shape when it fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& e : edges) {
    fillet.Add(r, e);
  }
  return build(fillet) ? fillet.Shape() : TopoDS_Shape();
}

// A boss 41.77 x 9 x 1.5 on a wall (the plane z = 0), rounded as a stored
// design has it: its long top edges with 2 (wider than its sides: the
// rounding rolls on the foot edges, a cylinder), the foot of those
// roundings with 2 (a concave cylinder of radius 2 tangent to the wall
// along a line), then the ends' edges with the roundings (and on along the
// ends' top edges, tangent to them) with 1.5: at each corner the foot
// rounding's corner is a torus of major radius 3.5 about the foot
// cylinder's axis. A null shape when a step fails.
TopoDS_Shape boss_with_rounded_long_sides() {
  const double       L = 41.77, W = 9.0, H = 1.5, t = 1e-6;
  const TopoDS_Shape wall = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -10), gp_Pnt(L + 10, W + 10, 0)).Shape();
  const TopoDS_Shape boss = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(L, W, H)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(wall, boss).Shape());
  unify.Build();
  TopoDS_Shape body  = unify.Shape();
  const auto   along = [&](const gp_Pnt& m, double z) {
    return std::abs(m.Z() - z) < t && m.X() > 0 && m.X() < L && m.Y() > -1 && m.Y() < W + 1;
  };
  body = rounded(body, edges_where(body, [&](const gp_Pnt& m) { return along(m, H); }), 2.0);
  if (body.IsNull()) {
    return body;
  }
  body = rounded(body, edges_where(body, [&](const gp_Pnt& m) { return along(m, 0.0); }), 2.0);
  if (body.IsNull()) {
    return body;
  }
  return rounded(body,
                 edges_where(body,
                             [&](const gp_Pnt& m) {
                               return (std::abs(m.X()) < t || std::abs(m.X() - L) < t) && m.Z() > t &&
                                      m.Z() < H - t;
                             }),
                 1.5);
}

// The ends' feet of that boss rounded with 2 (port patch 0460), the radius
// of the foot roundings beside them: at each corner the ball fits the foot
// rounding, so the end section is that rounding's end arc, from the
// corner's vertex on the wall to the arc's other end, where the opposite
// contact ends; the blend touches the rounding all along it and consumes
// the corner torus between them. OCCT's corner (OnSame) intersected the
// blend with the torus, tangent to it all along that arc, and failed. Now
// the cap is the arc, the torus goes, and the new faces join the foot
// roundings tangentially along their arcs.
void test_foot_rounding_ending_on_a_rounding_of_its_radius() {
  const TopoDS_Shape body = boss_with_rounded_long_sides();
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const double L = 41.77, W = 9.0, t = 1e-6;
  const auto   is_corner_torus = [](const TopoDS_Face& f) {
    const BRepAdaptor_Surface s(f);
    return s.GetType() == GeomAbs_Torus && std::abs(s.Torus().MajorRadius() - 3.5) < 1e-6;
  };
  int tori = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    tori += is_corner_torus(TopoDS::Face(f.Current())) ? 1 : 0;
  }
  CHECK(tori == 4);
  const std::vector<TopoDS_Edge> feet = edges_where(body, [&](const gp_Pnt& m) {
    return std::abs(m.Z()) < t && ((m.X() > -1 && m.X() < 3) || (m.X() < L + 1 && m.X() > L - 3)) && m.Y() > -2 &&
           m.Y() < W + 2;
  });
  CHECK(feet.size() == 10);
  const TopoDS_Shape result = rounded(body, feet, 2.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // A concave rounding adds material only.
  CHECK(volume_of(result) - volume_of(body) > 4.0);
  CHECK(volume_of(BRepAlgoAPI_Cut(body, result).Shape()) < 1e-9);
  // The corner tori are consumed; each foot cylinder's end arc at a corner
  // (in the plane x = 1.5 or x = L - 1.5) bounds a new face tangent to it.
  int tori_left = 0, arcs = 0;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edge_faces;
  TopExp::MapShapesAndAncestors(result, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  for (int i = 1; i <= edge_faces.Extent(); ++i) {
    const TopoDS_Edge&       e = TopoDS::Edge(edge_faces.FindKey(i));
    const BRepAdaptor_Curve c(e);
    if (c.GetType() != GeomAbs_Circle || std::abs(c.Circle().Radius() - 2.0) > 1e-6 ||
        std::abs(c.Circle().Location().Z() - 2.0) > 1e-6 || edge_faces(i).Extent() != 2) {
      continue;
    }
    const double x = c.Circle().Location().X();
    if (std::abs(x - 1.5) > 1e-6 && std::abs(x - (L - 1.5)) > 1e-6) {
      continue;
    }
    const TopoDS_Face& f1 = TopoDS::Face(edge_faces(i).First());
    const TopoDS_Face& f2 = TopoDS::Face(edge_faces(i).Last());
    const bool         c1 = BRepAdaptor_Surface(f1).GetType() == GeomAbs_Cylinder;
    const bool         c2 = BRepAdaptor_Surface(f2).GetType() == GeomAbs_Cylinder;
    if (c1 == c2) {
      continue;
    }
    ++arcs;
    // The normals of both faces at the arc's middle agree.
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    gp_Dir       n[2];
    int          k = 0;
    for (const TopoDS_Face* f : {&f1, &f2}) {
      const occ::handle<Geom_Surface> s = BRep_Tool::Surface(*f);
      GeomAPI_ProjectPointOnSurf      p(m, s);
      double                          u = 0, v = 0;
      p.LowerDistanceParameters(u, v);
      gp_Pnt pt;
      gp_Vec du, dv;
      s->D1(u, v, pt, du, dv);
      n[k++] = gp_Dir(du.Crossed(dv));
    }
    CHECK(std::abs(std::abs(n[0].Dot(n[1])) - 1.0) < 1e-6);
  }
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    tori_left += is_corner_torus(TopoDS::Face(f.Current())) ? 1 : 0;
  }
  CHECK(tori_left == 0);
  CHECK(arcs == 4);
}

// A copy of `body` whose conical faces lie on their surface trimmed to the
// faces' parameter bounds, as the cones of stored bodies do (the same
// parameters, the p-curves put on the trimmed surface, both of a seam's).
TopoDS_Shape with_trimmed_cones(const TopoDS_Shape& body) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(body).Shape();
  BRep_Builder       bb;
  for (TopExp_Explorer f(copy, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current().Oriented(TopAbs_FORWARD));
    if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Cone) {
      continue;
    }
    struct Curves {
      TopoDS_Edge                    edge;
      occ::handle<Geom2d_Curve> c1, c2;
      double                         first = 0., last = 0.;
    };
    std::vector<Curves> curves;
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge edge = TopoDS::Edge(e.Current());
      Curves            c;
      c.edge = edge;
      c.c1   = BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD)), face, c.first, c.last);
      if (BRep_Tool::IsClosed(edge, face)) {
        c.c2 = BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Oriented(TopAbs_REVERSED)), face, c.first, c.last);
      }
      curves.push_back(c);
    }
    double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
    BRepTools::UVBounds(face, u1, u2, v1, v2);
    TopLoc_Location                 loc;
    const occ::handle<Geom_Surface> s   = BRep_Tool::Surface(face, loc);
    const double                    tol = BRep_Tool::Tolerance(face);
    bb.UpdateFace(face, new Geom_RectangularTrimmedSurface(s, u1, u2, v1, v2), loc, tol);
    for (const Curves& c : curves) {
      if (c.c2.IsNull()) {
        bb.UpdateEdge(c.edge, c.c1, face, BRep_Tool::Tolerance(c.edge));
      } else {
        bb.UpdateEdge(c.edge, c.c1, c.c2, face, BRep_Tool::Tolerance(c.edge));
      }
      bb.Range(c.edge, face, c.first, c.last);
    }
  }
  return copy;
}

// A plate 60 x 60 x 10 on a conical foot below it: the cone's axis is the
// vertical through (30, 3), its radius 10 at the plate's bottom (z = 0) and
// 5 at z = -5 (45 degrees), cut flush with the plate's side y = 0. The
// plate's bottom edge along that side runs from x = 0 to the foot's rim at
// x = 30 - sqrt(91).
TopoDS_Shape plate_on_a_conical_foot() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(60, 60, 10)).Shape();
  const TopoDS_Shape cone =
    BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(30, 3, -5), gp_Dir(0, 0, 1)), 5.0, 10.0, 5.0).Shape();
  const TopoDS_Shape beside = BRepPrimAPI_MakeBox(gp_Pnt(10, -20, -10), gp_Pnt(50, 0, 1)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, BRepAlgoAPI_Cut(cone, beside).Shape()).Shape());
  unify.Build();
  return unify.Shape();
}

// The plate's bottom edge along the side y = 0 rounded with 3 up to the
// foot (port patch 0490). The side's contact, 3 above the bottom, meets the
// foot's cone only above the cone's rim (where the cone continued would
// cross it, at x = 30 - sqrt(160), before the edge's end at the rim), and
// the bottom's contact meets the rim. OCCT's corner (OnSame: the cone is the
// face at the end) intersects the side's contact with the cone, which a cone
// on a trimmed surface, as stored bodies have them, bounds at its rim: no
// intersection, no cap ("bouchon non ecrit"). Now the cone is taken as its
// surface continues, as it is when that surface is not trimmed: the cap is
// the blend's intersection with the cone, and the cone's face grows up into
// the rounding's end. The result is the rounding of the corner region
// outside the cone continued above the plate's bottom.
void test_rounding_ending_at_a_trimmed_cone_past_its_rim() {
  const TopoDS_Shape plain = plate_on_a_conical_foot();
  const double       xv    = 30 - std::sqrt(91.0), r = 3.0;
  const auto         bottom_side = [&](const TopoDS_Shape& s) {
    return edges_where(s, [&](const gp_Pnt& m) {
      return std::abs(m.Y()) < 1e-6 && std::abs(m.Z()) < 1e-6 && m.X() > 0 && m.X() < xv;
    });
  };
  CHECK(bottom_side(plain).size() == 1);
  // Not trimmed, OCCT's corner builds it; trimmed, only with the patch.
  const TopoDS_Shape trimmed = with_trimmed_cones(plain);
  CHECK(BRepCheck_Analyzer(trimmed).IsValid());
  const TopoDS_Shape a = rounded(plain, bottom_side(plain), r);
  const TopoDS_Shape b = rounded(trimmed, bottom_side(trimmed), r);
  CHECK(!a.IsNull());
  CHECK(!b.IsNull());
  if (a.IsNull() || b.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(b).IsValid());
  // The corner region along the edge (a 3 x 3 square less the quarter disc
  // of the ball's section), from the plate's end x = 0 to the cone's axis,
  // less the cone continued up to z = 5.
  BRepBuilderAPI_MakeWire section;
  const gp_Pnt            p0(0, 0, 0), p1(0, r, 0), p2(0, 0, r);
  section.Add(BRepBuilderAPI_MakeEdge(p0, p1).Edge());
  section.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p1, gp_Pnt(0, r - r / std::sqrt(2.0), r - r / std::sqrt(2.0)), p2).Value()).Edge());
  section.Add(BRepBuilderAPI_MakeEdge(p2, p0).Edge());
  const TopoDS_Shape corner = BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(section.Wire()).Face(), gp_Vec(30, 0, 0)).Shape();
  const TopoDS_Shape cone_on =
    BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(30, 3, -5), gp_Dir(0, 0, 1)), 5.0, 15.0, 10.0).Shape();
  const double removed = volume_of(BRepAlgoAPI_Cut(corner, cone_on).Shape());
  CHECK(removed > 10.0);
  CHECK(std::abs(volume_of(trimmed) - volume_of(b) - removed) < 1e-6 * removed);
  CHECK(std::abs(volume_of(a) - volume_of(b)) < 1e-6 * removed);
  CHECK(face_count(a) == face_count(b));
  // The cone's face reaches up into the rounding's end, past its rim.
  double top = -1.0;
  for (TopExp_Explorer f(b, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() != GeomAbs_Cone) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    top = std::max(top, z1);
  }
  CHECK(top > 2.0);
}

// A wall x in [-4, 0], y in [0, 10], z in [0, 5] on a floor (z < 0), its
// end y < 0 a half round of radius 2 about the vertical through (-2, 0):
// the wall's side x = 0 runs on tangentially into the half round at y = 0.
TopoDS_Shape wall_with_a_rounded_end() {
  const TopoDS_Shape floor = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -2), gp_Pnt(6, 14, 0)).Shape();
  const TopoDS_Shape side  = BRepPrimAPI_MakeBox(gp_Pnt(-4, 0, 0), gp_Pnt(0, 10, 5)).Shape();
  const TopoDS_Shape end   = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-2, 0, 0), gp_Dir(0, 0, 1)), 2.0, 5.0).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(floor, side).Shape(), end).Shape());
  unify.Build();
  return unify.Shape();
}

// The wall's foot rounded with r all round its end and along both sides,
// then that rounding taken off in front of the side x = 0 (the box
// x in [0, 1] cut away): the side's foot is sharp from y = -sqrt(2 r + r r)
// - here -1.5 for r = 0.5 - where the earlier rounding's floor contact (a
// circle of radius 2 + r about the end's axis) crosses the side's plane, the
// earlier rounding's tube cut by that plane up to where it touches it at
// y = 0. With `spline_edges` the earlier rounding's edges on the floor carry
// their circles as B-splines bounded at their vertices, as stored bodies do.
// The tangent edges are coded as such (BRepLib::EncodeRegularity).
TopoDS_Shape wall_foot_cut_back(double r, bool spline_edges, TopoDS_Shape& rounded_all_round) {
  const TopoDS_Shape wall = wall_with_a_rounded_end();
  rounded_all_round       = rounded(wall, edges_where(wall, [](const gp_Pnt& m) {
                                return std::abs(m.Z()) < 1e-7 && m.X() > -4.0 - 1e-7 && m.X() < 1e-7 && m.Y() > -2.0 - 1e-7
                                       && m.Y() < 10.0 - 1e-7;
                              }),
                              r);
  if (rounded_all_round.IsNull()) {
    return TopoDS_Shape();
  }
  const TopoDS_Shape           front = BRepPrimAPI_MakeBox(gp_Pnt(0, -3, 0), gp_Pnt(1, 11, 1)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(rounded_all_round, front).Shape());
  unify.Build();
  TopoDS_Shape body = unify.Shape();
  BRepLib::EncodeRegularity(body, 1e-6);
  if (spline_edges) {
    BRep_Builder bb;
    for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge       edge = TopoDS::Edge(e.Current());
      const BRepAdaptor_Curve c(edge);
      if (c.GetType() != GeomAbs_Circle || std::abs(c.Circle().Radius() - (2.0 + r)) > 1e-6
          || std::abs(c.Value(c.FirstParameter()).Z()) > 1e-6) {
        continue;
      }
      double                        f = 0., l = 0.;
      const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, f, l);
      bb.UpdateEdge(edge, GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, f, l)),
                    BRep_Tool::Tolerance(edge));
      bb.Range(edge, f, l, true);
      bb.SameParameter(edge, false);
      BRepLib::SameParameter(edge, 1e-7);
    }
  }
  return body;
}

// A bar 10 x 10 (y from -13 to -3, z from 5 to 15) joined to a cylinder of
// radius 25.25 about the z axis (cut off at y = 20), its long edges at
// y = -3 chamfered by 2 and the chamfers' edges with that side rounded with
// 5, as a stored design has it. The cap's edge with the cylinder keeps the
// vertex where the bar's side met it before the chamfer, with only the two
// pieces of that edge at it; with `joined` the pieces are one edge. A null
// shape when a step fails.
TopoDS_Shape bar_on_a_cylinder(bool joined) {
  const double R = 25.25, t = 1e-6;
  TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1)), R, 10).Shape();
  cylinder = BRepAlgoAPI_Cut(cylinder, BRepPrimAPI_MakeBox(gp_Pnt(-100, 20, 0), gp_Pnt(100, 100, 20)).Shape()).Shape();
  const TopoDS_Shape bar = BRepPrimAPI_MakeBox(gp_Pnt(-92, -13, 5), gp_Pnt(-R + 5, -3, 15)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(cylinder, bar).Shape(), true, true, true);
  unify.Build();
  TopoDS_Shape body = unify.Shape();
  BRepFilletAPI_MakeChamfer chamfer(body);
  for (const TopoDS_Edge& e : edges_where(body, [&](const gp_Pnt& m) {
         return std::abs(m.Y() + 3) < t && (std::abs(m.Z() - 15) < t || std::abs(m.Z() - 5) < t) && m.X() < -30;
       })) {
    chamfer.Add(2.0, e);
  }
  chamfer.Build();
  if (!chamfer.IsDone()) {
    return TopoDS_Shape();
  }
  body = rounded(chamfer.Shape(), edges_where(chamfer.Shape(), [&](const gp_Pnt& m) {
                   return std::abs(m.Y() + 3) < t && (std::abs(m.Z() - 13) < t || std::abs(m.Z() - 7) < t) && m.X() < -30;
                 }),
                 5.0);
  if (joined && !body.IsNull()) {
    ShapeUpgrade_UnifySameDomain join(body, true, false, true);
    join.Build();
    body = join.Shape();
  }
  return body;
}

// The cut-back side's foot rounded with the earlier rounding's radius r
// (port patch 0540). Its spine ends at the vertex where the earlier
// rounding's floor contact crosses the side's plane; there that rounding is
// the face at the end (OnSame), and the new blend's section at y = 0, where
// the two tubes' spines are tangent, lies in the earlier tube. OCCT's corner
// intersected the blend's floor contact with that tube, which it only
// touches, and then the blend with the tube, tangent to it all along the
// section: no cap. Now the cap is that section, the floor contact's end the
// point where it touches the tube, and the earlier rounding's edge on the
// floor is prolonged from the vertex to that point (along its circle, or,
// where its curve is a B-spline ending at the vertex, along the tube's iso
// it runs on). The result is the wall rounded all round in one go: the
// earlier rounding grows back up to the section.
void test_rounding_running_on_into_an_earlier_roundings_tube() {
  const double r = 0.5;
  for (const bool spline_edges : {false, true}) {
    TopoDS_Shape       all_round;
    const TopoDS_Shape body = wall_foot_cut_back(r, spline_edges, all_round);
    CHECK(!body.IsNull());
    if (body.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(body).IsValid());
    const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& m) {
      return std::abs(m.X()) < 1e-7 && std::abs(m.Z()) < 1e-7 && m.Y() > -1.5 && m.Y() < 10.0;
    });
    CHECK(foot.size() == 1);
    if (foot.size() != 1) {
      return;
    }
    const gp_Pnt start = BRep_Tool::Pnt(TopExp::FirstVertex(foot[0]));
    const gp_Pnt end   = BRep_Tool::Pnt(TopExp::LastVertex(foot[0]));
    CHECK(std::abs(std::min(start.Y(), end.Y()) + 1.5) < 1e-6);
    const TopoDS_Shape result = rounded(body, foot, r);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    // The volume of the wall rounded all round, its new blend one face.
    const double expected = volume_of(all_round);
    CHECK(std::abs(volume_of(result) - expected) < 1e-4 * (expected - volume_of(body)));
    CHECK(face_count(result) == face_count(body) + 1);
  }
}

// The bar's concave edge with the cylinder rounded with 3 (port patch 0550):
// the rounding runs on tangentially over the side's roundings and chamfers to
// the cap at both ends. Its contact on the cylinder reaches the cap first,
// the other one then runs over the chamfer to the cap: the end lies over two
// pieces, cut by the cap, and the cylinder's contact ends on the cap's edge
// piece away from the vertex. OCCT took that piece for no edge at the vertex
// and failed ("the max number of faces reached"); now the end is the same
// as where that edge is one.
void test_rounding_ending_over_pieces_where_the_cap_edge_is_in_two() {
  const TopoDS_Shape split  = bar_on_a_cylinder(false);
  const TopoDS_Shape joined = bar_on_a_cylinder(true);
  CHECK(!split.IsNull());
  CHECK(!joined.IsNull());
  if (split.IsNull() || joined.IsNull()) {
    return;
  }
  // The vertex at the bar's former corner on the cap: two edges there.
  const auto corner_edges = [](const TopoDS_Shape& s) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
    TopExp::MapShapes(s, TopAbs_EDGE, edges);
    int n = 0;
    for (int i = 1; i <= edges.Extent(); ++i) {
      for (const TopoDS_Vertex& v : {TopExp::FirstVertex(TopoDS::Edge(edges(i))), TopExp::LastVertex(TopoDS::Edge(edges(i)))}) {
        n += BRep_Tool::Pnt(v).Distance(gp_Pnt(-std::sqrt(25.25 * 25.25 - 9.0), -3, 15)) < 1e-6 ? 1 : 0;
      }
    }
    return n;
  };
  CHECK(corner_edges(split) == 2);
  CHECK(corner_edges(joined) == 0);
  const auto concave = [](const TopoDS_Shape& s) {
    return edges_where(s, [](const gp_Pnt& m) { return std::abs(m.Y() + 3) < 1e-6 && std::abs(m.Z() - 10) < 1e-6 && m.X() > -30; });
  };
  CHECK(concave(split).size() == 1);
  const TopoDS_Shape a = rounded(split, concave(split), 3.0);
  const TopoDS_Shape b = rounded(joined, concave(joined), 3.0);
  CHECK(!a.IsNull());
  CHECK(!b.IsNull());
  if (a.IsNull() || b.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(a).IsValid());
  const double added = volume_of(a) - volume_of(split);
  CHECK(added > 20.0);
  CHECK(std::abs(added - (volume_of(b) - volume_of(joined))) < 1e-6 * added);
  CHECK(face_count(a) == face_count(b));
  // Every edge within the body's slab and of the walk's tolerance.
  for (TopExp_Explorer e(a, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    CHECK(BRep_Tool::Tolerance(edge) < 1e-4);
    Bnd_Box box;
    BRepBndLib::AddOptimal(edge, box, false, false);
    double x0, y0, z0, x1, y1, z1;
    box.Get(x0, y0, z0, x1, y1, z1);
    CHECK(z0 > 5 - 1e-4 && z1 < 15 + 1e-4);
  }
}

// A pocket under a ceiling (the plate z from 0 to 10) between two walls,
// x < 0 and x > 40, from y = -15 to 15, 18 deep; at both ends of each wall
// a block 2 long (y from 15 to 17 and from -17 to -15) whose side lies at
// 8 + d from its wall, where a rounding of 8 touches the ceiling. Ribs 1
// thick stand out 6 from both walls at y from 9 to 10 (ribs & 1) and from
// -10 to -9 (ribs & 2).
TopoDS_Shape pocket_with_ribs(double d, int ribs) {
  TopoDS_Shape s   = BRepPrimAPI_MakeBox(gp_Pnt(-20, -25, 0), gp_Pnt(60, 25, 10)).Shape();
  const auto   add = [&](const gp_Pnt& a, const gp_Pnt& b) {
    s = BRepAlgoAPI_Fuse(s, BRepPrimAPI_MakeBox(a, b).Shape()).Shape();
  };
  add(gp_Pnt(-20, -15, -18), gp_Pnt(0, 15, 0));
  add(gp_Pnt(40, -15, -18), gp_Pnt(60, 15, 0));
  for (const double y : {15.0, -17.0}) {
    add(gp_Pnt(-20, y, -18), gp_Pnt(8 + d, y + 2, 0));
    add(gp_Pnt(32 - d, y, -18), gp_Pnt(60, y + 2, 0));
  }
  for (const double y : {9.0, -10.0}) {
    if ((ribs & (y > 0 ? 1 : 2)) != 0) {
      add(gp_Pnt(0, y, -18), gp_Pnt(6, y + 1, 0));
      add(gp_Pnt(34, y, -18), gp_Pnt(40, y + 1, 0));
    }
  }
  ShapeUpgrade_UnifySameDomain unify(s, true, true, true);
  unify.Build();
  return unify.Shape();
}

// The walls' edges with the ceiling in the pocket above rounded with 8: the
// result and the volume it adds, against the rounding's section (64 - 16 pi
// along the walls; over a rib only its part beyond the rib's end, x > 6).
void check_pocket_rounded(double d, int ribs) {
  const TopoDS_Shape body = pocket_with_ribs(d, ribs);
  const TopoDS_Shape result =
    rounded(body, edges_where(body, [](const gp_Pnt& m) {
              return std::abs(m.Z()) < 1e-7 && (std::abs(m.X()) < 1e-7 || std::abs(m.X() - 40) < 1e-7) && std::abs(m.Y()) < 15;
            }),
            8.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const int    nr      = ((ribs & 1) != 0 ? 1 : 0) + ((ribs & 2) != 0 ? 1 : 0);
  const double section = 64 - 16 * M_PI;
  const double beyond  = 16 - (std::sqrt(60.0) + 32 * std::asin(0.25));
  const double added   = volume_of(result) - volume_of(body);
  CHECK_NEAR_TOL(added, 2 * (section * (30 - nr) + beyond * nr), 1e-6);
  // One cylinder along each wall.
  int cylinders = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    cylinders += BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cylinder ? 1 : 0;
  }
  CHECK(cylinders == 2);
}

// The roundings end at the blocks, whose sides lie where they touch the
// ceiling (port patch 0580): past the end the contact line runs along the
// block's concave edge with the ceiling, which the hatcher took as inside
// the ceiling or not as the line lay a rounding error to one side or the
// other. Inside, the contact ran on to the ceiling's far edge and the
// result was invalid; now it is off the ceiling there, whichever side of
// the edge the line lies (d of 1e-9 either way).
void test_rounding_ending_at_a_block_flush_with_its_contact() {
  for (const double d : {0.0, 1e-9, -1e-9}) {
    check_pocket_rounded(d, 0);
  }
}

// The walls' edges broken by the ribs (port patch 0581): the roundings on
// either side of a rib are joined over it, one cylinder along the wall cut
// by the rib, whose sides cut it along sections. The pieces met at the
// side's edge with the blend and the shell was open; with two ribs the
// stripe joined first kept a surface of its own.
void test_roundings_joined_across_ribs_with_square_sides() {
  for (const int ribs : {1, 2, 3}) {
    check_pocket_rounded(0.0, ribs);
  }
}

// The height of the plate's top at x: the plane z = 0 (radius 0) or a
// cylinder of that radius about the line x = 0, z = -radius along y.
double plate_top(double radius, double x) {
  return radius > 0 ? std::sqrt(radius * radius - x * x) - radius : 0.0;
}

// A plate from z = -6 up to its top N (plate_top) whose side Fo is the
// vertical plane through V = (0.5, 0, top) at 8 degrees to y (through the
// points x = 0.5 + y tan 8), from y = 0 to 12, and whose left side is the
// plane x = left. Past V (y < 0) a tongue fills the plate out to the plane
// B through V that holds the top's line x = 0.5 and leans in under it (an
// overhang, B's edge with Fo going down from V by 1 for every 4 along Fo):
// B's edge with the top is convex, with Fo concave. Also returns the half
// space on the tongue's side of B.
TopoDS_Shape plate_with_overhanging_tongue(double radius, double left, TopoDS_Shape& tongue_side,
                                           double left_round = 0.0) {
  const double a = 8.0 * M_PI / 180.0, s = 0.25;
  const gp_Pnt v(0.5, 0, plate_top(radius, 0.5));
  const gp_Dir n2(std::cos(a), -std::sin(a), 0); // out of Fo
  // B holds the top's line x = 0.5 (along y) and the edge with Fo down
  // from V.
  const gp_Vec ruling(0, 1, 0);
  const gp_Vec down(-std::sin(a), -std::cos(a), -s);
  const gp_Dir nb(ruling.Crossed(down).Reversed()); // out of B, away from the tongue
  // A half space as a box 200 wide whose face lies on the plane through p
  // with outward normal n (the box on the side n points to).
  const auto half_space = [](const gp_Pnt& p, const gp_Dir& n, const gp_Dir& x) {
    const gp_Dir y      = n.Crossed(x);
    const gp_Pnt corner = p.Translated(-100.0 * gp_Vec(x) - 100.0 * gp_Vec(y));
    return BRepPrimAPI_MakeBox(gp_Ax2(corner, n, x), 200, 200, 200).Shape();
  };
  TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(left, -8, -6), gp_Pnt(6, 12, radius > 0 ? 1 : 0)).Shape();
  if (radius > 0) {
    box = BRepAlgoAPI_Common(
            box, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -20, -radius), gp_Dir(0, 1, 0)), radius, 40).Shape())
            .Shape();
  }
  const TopoDS_Shape beyond = half_space(v, n2, gp_Dir(0, 0, 1));
  const TopoDS_Shape past   = BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, -50), gp_Pnt(50, 0, 50)).Shape();
  tongue_side               = half_space(v, nb.Reversed(), gp_Dir(0, -1, 0));
  const TopoDS_Shape plate  = BRepAlgoAPI_Cut(box, beyond).Shape();
  const TopoDS_Shape tongue = BRepAlgoAPI_Common(
    BRepAlgoAPI_Common(BRepAlgoAPI_Common(box, beyond).Shape(), tongue_side).Shape(), past).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, tongue).Shape(), true, true, true);
  unify.Build();
  if (left_round <= 0) {
    return unify.Shape();
  }
  // The left side's edge with the top rounded.
  const TopoDS_Shape body = unify.Shape();
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(e.Current()));
    const gp_Pnt            m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(m.X() - left) < 1e-7 && std::abs(m.Z() - plate_top(radius, left)) < 1e-6) {
      fillet.Add(left_round, TopoDS::Edge(e.Current()));
    }
  }
  fillet.Build();
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

// The plate's edge between its top and Fo rounded with 1: the volume
// removed, or -1 when the fillet fails or its result is invalid.
double plate_side_rounding_removes(double radius, double left, double left_round = 0.0) {
  TopoDS_Shape       tongue_side;
  const TopoDS_Shape body = plate_with_overhanging_tongue(radius, left, tongue_side, left_round);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return -1.0;
  }
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const double t = std::tan(8.0 * M_PI / 180.0);
  const std::vector<TopoDS_Edge> edge = edges_where(body, [t, radius](const gp_Pnt& m) {
    return m.Y() > 0 && std::abs(m.X() - 0.5 - m.Y() * t) < 1e-7 && std::abs(m.Z() - plate_top(radius, m.X())) < 1e-6;
  });
  CHECK(edge.size() == 1);
  if (edge.size() != 1) {
    return -1.0;
  }
  const TopoDS_Shape result = rounded(body, edge, 1.0);
  if (result.IsNull() || !BRepCheck_Analyzer(result).IsValid()) {
    return -1.0;
  }
  return volume_of(body) - volume_of(result);
}

// The edge between the plate's top and Fo rounded with 1 (port patch
// 0600): an OnSame end at V whose blend runs on past V along Fo, cut by B
// along its side from where its contact on the top crosses B (7 before V)
// to where its contact on Fo reaches B's edge with Fo (4 past V); B grows
// up to the cut. On the cylindrical top the blend is walked. Where the
// tongue's left edge is rounded with 0.3 (its side x = -1 or -0.9), the
// contact on the top runs off the top onto that rounding 1.3 or 0.6 past V,
// a face tangent to the top, so OCCT ended the blend there, short of B's
// edge with Fo, and the corner took the rounding for the face at the end:
// an invalid result. Now the blend is walked again with the top unbounded
// up to B's edge with Fo, and the corner cuts it by B: the volume removed is
// that of a tongue wide enough for the contact (left side x = -3) or with
// a sharp left edge (where OCCT's walk ran on past the edge already). On
// the plane top (an analytic blend, not walked) the volume removed is the
// rounding's along the edge up to the end y = 12, less its part on the
// tongue's side of B.
void test_rounding_cut_along_its_side_by_the_face_at_an_onsame_end() {
  const double wide = plate_side_rounding_removes(50.0, -3.0);
  CHECK(wide > 1.0);
  CHECK_NEAR_TOL(plate_side_rounding_removes(50.0, -0.7), wide, 1e-6);
  CHECK_NEAR_TOL(plate_side_rounding_removes(50.0, -1.0, 0.3), wide, 1e-6);
  CHECK_NEAR_TOL(plate_side_rounding_removes(50.0, -0.9, 0.3), wide, 1e-6);
  TopoDS_Shape tongue_side;
  plate_with_overhanging_tongue(0.0, -0.7, tongue_side);
  const double       t = std::tan(8.0 * M_PI / 180.0);
  const gp_Dir       in_top(-std::cos(8.0 * M_PI / 180.0), std::sin(8.0 * M_PI / 180.0), 0);
  const TopoDS_Shape section =
    rounded_away(gp_Pnt(0.5, 0, 0), gp_Pnt(0.5 + 12 * t, 12, 0), in_top, gp_Dir(0, 0, -1), 1.0);
  const TopoDS_Shape up_to_end = BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, -50), gp_Pnt(50, 12, 50)).Shape();
  const double       expected =
    volume_of(BRepAlgoAPI_Common(BRepAlgoAPI_Cut(section, tongue_side).Shape(), up_to_end).Shape());
  CHECK(expected > 1.0);
  CHECK_NEAR_TOL(plate_side_rounding_removes(0.0, -0.7), expected, 1e-5);
}

// The same rounding where its last piece is walked again past V (the
// tongue's left edge rounded with 0.3, port patch 0600): the face of that
// piece is generated by the rounded edge as the other pieces' faces are
// (port patch 0900; it was left out, so that a caller naming the result's
// faces after the edges they come from found it unnamed): every face of
// the result on a surface no face of the body has is generated by the edge
// or by one of its vertices.
void test_rounding_walked_again_past_an_onsame_end_generated_by_its_edge() {
  TopoDS_Shape       tongue_side;
  const TopoDS_Shape body = plate_with_overhanging_tongue(50.0, -1.0, tongue_side, 0.3);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const double                   t    = std::tan(8.0 * M_PI / 180.0);
  const std::vector<TopoDS_Edge> edge = edges_where(body, [t](const gp_Pnt& m) {
    return m.Y() > 0 && std::abs(m.X() - 0.5 - m.Y() * t) < 1e-7 && std::abs(m.Z() - plate_top(50.0, m.X())) < 1e-6;
  });
  CHECK(edge.size() == 1);
  if (edge.size() != 1) {
    return;
  }
  BRepFilletAPI_MakeFillet fillet(body);
  fillet.Add(1.0, edge.front());
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape&                                            result = fillet.Shape();
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> generated;
  for (const TopoDS_Shape& f : fillet.Generated(edge.front())) {
    generated.Add(f);
  }
  for (const TopoDS_Shape& v : {TopoDS_Shape(TopExp::FirstVertex(edge.front())),
                                TopoDS_Shape(TopExp::LastVertex(edge.front()))}) {
    for (const TopoDS_Shape& f : fillet.Generated(v)) {
      generated.Add(f);
    }
  }
  std::vector<const Geom_Surface*> surfaces;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    surfaces.push_back(BRep_Tool::Surface(TopoDS::Face(f.Current())).get());
  }
  int added = 0, named = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const Geom_Surface* s = BRep_Tool::Surface(TopoDS::Face(f.Current())).get();
    if (std::find(surfaces.begin(), surfaces.end(), s) != surfaces.end()) {
      continue;
    }
    ++added;
    if (generated.Contains(f.Current())) {
      ++named;
    }
  }
  CHECK(added >= 1);
  CHECK(named == added);
}

// A wall (the plane x = 0 for z in [0, 5]) standing on a floor (z = 0, y in
// [0, 10]) that narrows: its far edge, where a cliff drops to z = -3, is
// one cubic B-spline, along x = 2 up to y = 0.9, turning in between, along
// x = w from y = 4.6 to the end y = 10. All faces are B-splines (the blend
// is walked, as between the faces of stored bodies).
TopoDS_Shape wall_on_a_narrowing_floor(double w) {
  const TopoDS_Shape         wall  = BRepPrimAPI_MakeBox(gp_Pnt(-5, 0, -4), gp_Pnt(0, 10, 5)).Shape();
  const TopoDS_Shape         floor = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -4), gp_Pnt(5, 10, 0)).Shape();
  const double               xs[8] = {2, 2, 2, 2, w, w, w, w};
  const double               ys[8] = {-1, 0, 0.5, 1.5, 3, 4, 7, 11};
  NCollection_Array1<gp_Pnt> poles(1, 8);
  for (int i = 0; i < 8; ++i) {
    poles(i + 1) = gp_Pnt(xs[i], ys[i], -3);
  }
  NCollection_Array1<double> knots(1, 6);
  NCollection_Array1<int>    mults(1, 6);
  for (int i = 1; i <= 6; ++i) {
    knots(i) = i - 1;
    mults(i) = i == 1 || i == 6 ? 4 : 1;
  }
  BRepBuilderAPI_MakeWire outline;
  outline.Add(
    BRepBuilderAPI_MakeEdge(occ::handle<Geom_Curve>(new Geom_BSplineCurve(poles, knots, mults, 3))).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(w, 11, -3), gp_Pnt(6, 11, -3)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(6, 11, -3), gp_Pnt(6, -1, -3)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(6, -1, -3), gp_Pnt(2, -1, -3)).Edge());
  const TopoDS_Shape beyond =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire(), true).Face(), gp_Vec(0, 0, 4)).Shape();
  ShapeUpgrade_UnifySameDomain unify(
    BRepAlgoAPI_Cut(BRepAlgoAPI_Fuse(wall, floor).Shape(), beyond).Shape(), true, true, true);
  unify.Build();
  BRepBuilderAPI_NurbsConvert nurbs(unify.Shape(), true);
  return nurbs.Shape();
}

// The area of the rounding's section (radius 1) between the wall and a
// floor of width w at the wall's foot: the ball touches the wall and, where
// the floor is narrower than 1, rolls on the floor's far edge.
double wall_foot_section(double w) {
  if (w >= 1.0) {
    return 1.0 - M_PI / 4.0;
  }
  const double h = std::sqrt(2.0 * w - w * w); // the ball's centre above the floor
  return h - 0.5 * (h * (1.0 - w) + std::asin(h));
}

// The wall's foot rounded with 1 (port patch 0620): the floor's contact
// leaves the floor across its far edge where that turns in (y 2 to 3),
// long before the end of the spine, and the faces at the end vertex (the
// wall, the floor and the end y = 10) do not include the cliff beyond that
// edge. OCCT took that edge for the end region (it is a few edges from the
// end vertex) and the rounding stopped there, the rest of the edge left
// sharp. Now the ball rolls on the far edge from there, as in the middle of
// a rounding, up to the end, where the end face cuts it: the section is the
// full rounding's where the floor is wide and the rolling ball's where it
// is narrow, nothing added beyond the floor's edge.
void test_rounding_rolls_on_the_far_edge_of_a_floor_narrowing_before_the_end() {
  const double       w    = 0.5;
  const TopoDS_Shape body = wall_on_a_narrowing_floor(w);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> foot =
    edges_where(body, [](const gp_Pnt& m) { return std::abs(m.X()) < 1e-7 && std::abs(m.Z()) < 1e-7; });
  CHECK(foot.size() == 1);
  if (foot.size() != 1) {
    return;
  }
  const TopoDS_Shape result = rounded(body, foot, 1.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const TopoDS_Shape added = BRepAlgoAPI_Cut(result, body).Shape();
  CHECK(volume_of(BRepAlgoAPI_Cut(body, result).Shape()) < 1e-9);
  const auto in_box = [&added](double x0, double y0, double x1, double y1) {
    return volume_of(
      BRepAlgoAPI_Common(added, BRepPrimAPI_MakeBox(gp_Pnt(x0, y0, -10), gp_Pnt(x1, y1, 10)).Shape()).Shape());
  };
  CHECK_NEAR_TOL(in_box(-10, 0, 10, 1), wall_foot_section(2.0), 1e-4);
  CHECK_NEAR_TOL(in_box(-10, 5, 10, 10), 5.0 * wall_foot_section(w), 5e-4);
  CHECK(in_box(w + 1e-6, 4, 10, 10) < 1e-9);
}

// A bar x in [0, 20], y in [0, 10], z in [0, 10] whose front edge along the
// floor (y = 10, z = 0) is rounded with 3, and beside it a block x in
// [20, 24] reaching out to y = 15 + d whose front edge along the floor is
// rounded with 8: the block's floor goes d further than the bar's (to
// y = 7 + d against 7). The step between the two profiles, the block's face
// x = 20, narrows to a point where the bar's rounding meets the floor. Also
// the block alone and the bar's profile run on along x through both, for
// the expected rounding of the step's foot.
struct StepBesideRoundedBar {
  TopoDS_Shape body, block, bar_profile;
};

TopoDS_Shape with_front_floor_edge_rounded(const TopoDS_Shape& box, double y, double r) {
  BRepFilletAPI_MakeFillet fillet(box);
  for (const TopoDS_Edge& e :
       edges_where(box, [y](const gp_Pnt& m) { return std::abs(m.Y() - y) < 1e-9 && std::abs(m.Z()) < 1e-9; })) {
    fillet.Add(r, e);
  }
  return build(fillet) ? fillet.Shape() : TopoDS_Shape();
}

StepBesideRoundedBar step_beside_rounded_bar(double d) {
  StepBesideRoundedBar s;
  const TopoDS_Shape   bar = with_front_floor_edge_rounded(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 10, 10)).Shape(), 10, 3);
  s.block = with_front_floor_edge_rounded(BRepPrimAPI_MakeBox(gp_Pnt(20, 0, 0), gp_Pnt(24, 15 + d, 10)).Shape(), 15 + d, 8);
  s.bar_profile =
    with_front_floor_edge_rounded(BRepPrimAPI_MakeBox(gp_Pnt(-1, 0, 0), gp_Pnt(30, 10, 10)).Shape(), 10, 3);
  if (bar.IsNull() || s.block.IsNull() || s.bar_profile.IsNull()) {
    return s;
  }
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(bar, s.block).Shape(), true, true, true);
  unify.Build();
  s.body = unify.Shape();
  return s;
}

TopoDS_Shape mirrored_in_y(const TopoDS_Shape& shape) {
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)));
  return BRepBuilderAPI_Transform(shape, mirror, true).Shape();
}

// The foot of the step (port patch 1140): its edge with the floor, 0.0733
// long, rounded with 0.5, on tangentially up the block's rounding and front.
// Near the bar the step is lower than the ball: the rounding's contact on it
// leaves it across its edge with the bar's rounding, and that rounding,
// tangent to the floor along the floor's edge with it, cuts the rounding all
// along its run on the block's rounding and floor up to the point where
// both meet that edge's line, 0.5 past the step (OCCT found no point where
// the block's floor edge crosses the bar's rounding, 0.9 micrometres above
// it, and failed: "the edge beside the end does not cross the face at the
// end"). The bar's rounding grows up to the cut, its edge with the floor
// prolonged along its line. The rounding takes what the block alone loses to
// the same rounding of its whole face x = 20, outside the bar's profile run
// on through the block; in both orientations of the chain (the body and its
// mirror image).
void test_step_foot_rounded_where_a_bar_rounding_tangent_to_the_floor_ends_it() {
  const double               d = 0.0733;
  const StepBesideRoundedBar s = step_beside_rounded_bar(d);
  CHECK(!s.body.IsNull());
  if (s.body.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(s.body).IsValid());
  const auto foot_of = [](const TopoDS_Shape& shape, double y0, double y1) {
    return edges_where(shape, [y0, y1](const gp_Pnt& m) {
      return std::abs(m.X() - 20) < 1e-9 && std::abs(m.Z()) < 1e-9 && m.Y() > std::min(y0, y1) && m.Y() < std::max(y0, y1);
    });
  };
  // The expected rounding: the block alone rounded along its face x = 20,
  // less what lies in the bar's profile.
  const std::vector<TopoDS_Edge> block_foot = foot_of(s.block, 0, 7 + d);
  CHECK(block_foot.size() == 1);
  const TopoDS_Shape block_rounded = rounded(s.block, block_foot, 0.5);
  CHECK(!block_rounded.IsNull());
  if (block_rounded.IsNull()) {
    return;
  }
  const TopoDS_Shape removed =
    BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(s.block, block_rounded).Shape(), s.bar_profile).Shape();
  const double removed_volume = volume_of(removed);
  CHECK(removed_volume > 0.5 && removed_volume < 1.5);
  for (const bool mirror : {false, true}) {
    const TopoDS_Shape body     = mirror ? mirrored_in_y(s.body) : s.body;
    const TopoDS_Shape expected = BRepAlgoAPI_Cut(body, mirror ? mirrored_in_y(removed) : removed).Shape();
    const double       sign     = mirror ? -1.0 : 1.0;
    const std::vector<TopoDS_Edge> foot = foot_of(body, sign * 7, sign * (7 + d));
    CHECK(foot.size() == 1);
    if (foot.size() != 1) {
      continue;
    }
    const TopoDS_Shape result = rounded(body, foot, 0.5);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    CHECK_NEAR_TOL(volume_of(body) - volume_of(result), removed_volume, 1e-6);
    CHECK(volume_of(BRepAlgoAPI_Cut(result, expected).Shape()) < 1e-6);
    CHECK(volume_of(BRepAlgoAPI_Cut(expected, result).Shape()) < 1e-6);
    // The bar's rounding past the step: its point 0.25 beyond it at y = 7.5
    // lies on the result's boundary.
    BRepClass3d_SolidClassifier classifier(result, gp_Pnt(20.25, sign * 7.5, 3 - std::sqrt(9 - 0.25)), 1e-6);
    CHECK(classifier.State() == TopAbs_ON);
  }
}

// A block x in [0, 24], y in [0, 20], z in [0, 20] whose back edge along the
// floor (y = 0, z = 0) is rounded with 13; for x up to 20 (the bar) a notch
// y < 5, z > 10 is taken out of its back and the notch's floor edge with the
// rounding rounded with 2, tangent to it. The block's face x = 20 beside the
// notch is a step whose foot on the big rounding ends where the bar's small
// rounding meets the big one, tangent to it. The block alone and the bar's
// profile run on along x through both, for the expected rounding.
struct StepBesideRoundedNotch {
  TopoDS_Shape body, block, bar_profile;
};

// A box x in [x0, x1], y in [0, 20], z in [0, 20], its back edge along the
// floor rounded with 13.
TopoDS_Shape with_back_floor_edge_rounded(double x0, double x1) {
  const TopoDS_Shape       box = BRepPrimAPI_MakeBox(gp_Pnt(x0, 0, 0), gp_Pnt(x1, 20, 20)).Shape();
  BRepFilletAPI_MakeFillet fillet(box);
  for (const TopoDS_Edge& e :
       edges_where(box, [](const gp_Pnt& m) { return std::abs(m.Y()) < 1e-9 && std::abs(m.Z()) < 1e-9; })) {
    fillet.Add(13, e);
  }
  return build(fillet) ? fillet.Shape() : TopoDS_Shape();
}

TopoDS_Shape notched_bar(double x0, double x1) {
  const TopoDS_Shape big = with_back_floor_edge_rounded(x0, x1);
  if (big.IsNull()) {
    return big;
  }
  const TopoDS_Shape notched =
    BRepAlgoAPI_Cut(big, BRepPrimAPI_MakeBox(gp_Pnt(x0 - 1, -1, 10), gp_Pnt(x1 + 1, 5, 21)).Shape()).Shape();
  const double                   y10  = 13 - std::sqrt(13 * 13 - 3 * 3); // the big rounding at z = 10
  const std::vector<TopoDS_Edge> edge = edges_where(notched, [y10](const gp_Pnt& m) {
    return std::abs(m.Y() - y10) < 1e-6 && std::abs(m.Z() - 10) < 1e-6;
  });
  return edge.size() == 1 ? rounded(notched, edge, 2) : TopoDS_Shape();
}

StepBesideRoundedNotch step_beside_rounded_notch() {
  StepBesideRoundedNotch s;
  s.block                = with_back_floor_edge_rounded(20, 24);
  const TopoDS_Shape bar = notched_bar(0, 20);
  s.bar_profile          = notched_bar(-1, 30);
  if (s.block.IsNull() || bar.IsNull() || s.bar_profile.IsNull()) {
    return s;
  }
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(bar, s.block).Shape(), true, true, true);
  unify.Build();
  s.body = unify.Shape();
  return s;
}

// The step's foot on the big rounding beside the notch rounded with 0.5, on
// tangentially up the block's back (port patch 1140). Near the bar the step
// is lower than the ball, and the bar's small rounding, tangent to the big
// one along their edge, cuts the rounding all along from where its contact
// on the step leaves the step up to the point where its contact on the big
// rounding meets that edge's line, 0.5 past the step. The analytic piece of
// the step's foot there, on the chain's end edge, still had the parameters
// of that edge's whole blend at its end; they are those of its end now. The
// rounding takes what the block alone loses to the same rounding of its
// face x = 20, outside the bar's profile run on through the block, in both
// orientations of the chain.
void test_step_foot_on_a_rounding_ended_by_a_rounding_tangent_to_it() {
  const StepBesideRoundedNotch s = step_beside_rounded_notch();
  CHECK(!s.body.IsNull());
  if (s.body.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(s.body).IsValid());
  // The step's edge with the big rounding: on x = 20, 13 from the rounding's
  // axis (beside the notch, above z = 9).
  const auto foot_of = [](const TopoDS_Shape& shape, double sign, double z0) {
    return edges_where(shape, [sign, z0](const gp_Pnt& m) {
      return std::abs(m.X() - 20) < 1e-9 && std::abs(std::hypot(sign * m.Y() - 13, m.Z() - 13) - 13) < 1e-6 &&
             m.Z() > z0 && m.Z() < 13;
    });
  };
  const std::vector<TopoDS_Edge> block_foot = foot_of(s.block, 1.0, 0.0);
  CHECK(block_foot.size() == 1);
  const TopoDS_Shape block_rounded = block_foot.empty() ? TopoDS_Shape() : rounded(s.block, block_foot, 0.5);
  CHECK(!block_rounded.IsNull());
  if (block_rounded.IsNull()) {
    return;
  }
  const TopoDS_Shape removed =
    BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(s.block, block_rounded).Shape(), s.bar_profile).Shape();
  const double removed_volume = volume_of(removed);
  CHECK(removed_volume > 0.1 && removed_volume < 3);
  for (const bool mirror : {false, true}) {
    const TopoDS_Shape             body     = mirror ? mirrored_in_y(s.body) : s.body;
    const TopoDS_Shape             expected = BRepAlgoAPI_Cut(body, mirror ? mirrored_in_y(removed) : removed).Shape();
    const std::vector<TopoDS_Edge> foot     = foot_of(body, mirror ? -1.0 : 1.0, 9.0);
    CHECK(foot.size() == 1);
    if (foot.size() != 1) {
      continue;
    }
    const TopoDS_Shape result = rounded(body, foot, 0.5);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    CHECK_NEAR_TOL(volume_of(body) - volume_of(result), removed_volume, 1e-5); // 1.45e-6 mm3 apart on Windows (MSVC)
    CHECK(volume_of(BRepAlgoAPI_Cut(result, expected).Shape()) < 1e-6);
    CHECK(volume_of(BRepAlgoAPI_Cut(expected, result).Shape()) < 1e-6);
  }
}

// A wall: the block x in [-20, 20], y in [-60, -40], z in [0, 10] outside
// the cylinder of radius 50 about the z axis; on its concave face a tab
// x in [-5, 5] up to y = -42, 3 high, its sides' edges with the wall rounded
// with 3 (each rounding tangent to the wall along a vertical line). The
// tab's top edge with the wall rounded with 1 (port patch 1140): its contact
// on the top leaves the top across the top's edges with the side roundings,
// which cut the rounding up to where its contact on the wall meets their
// lines of tangency with the wall, 1 above the top. The contacts' lines are
// circles about the z axis: the pieces' parameters at their ends keep to
// the pieces' periods. The rounding adds what lies under its torus within
// the tab's outline run on upwards.
void test_tab_top_rounded_up_to_its_side_roundings_tangent_to_a_curved_wall() {
  const double       R    = 50.0;
  const TopoDS_Shape bore = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), R, 12).Shape();
  const TopoDS_Shape wall = BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(-20, -60, 0), gp_Pnt(20, -40, 10)).Shape(), bore).Shape();
  ShapeUpgrade_UnifySameDomain unify(
    BRepAlgoAPI_Fuse(wall, BRepPrimAPI_MakeBox(gp_Pnt(-5, -55, 0), gp_Pnt(5, -42, 3)).Shape()).Shape(), true, true, true);
  unify.Build();
  const TopoDS_Shape joined = unify.Shape();
  // The tab's sides' vertical edges with the wall.
  const std::vector<TopoDS_Edge> sides = edges_where(joined, [R](const gp_Pnt& m) {
    return std::abs(std::abs(m.X()) - 5) < 1e-9 && std::abs(std::hypot(m.X(), m.Y()) - R) < 1e-6 && m.Z() > 0 &&
           m.Z() < 3;
  });
  CHECK(sides.size() == 2);
  const TopoDS_Shape body = sides.size() == 2 ? rounded(joined, sides, 3.0) : TopoDS_Shape();
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  // The tab's top edge with the wall: on the circle of radius R at z = 3
  // (in pieces where the fuse split it).
  const std::vector<TopoDS_Edge> top = edges_where(body, [R](const gp_Pnt& m) {
    return std::abs(m.Z() - 3) < 1e-9 && std::abs(std::hypot(m.X(), m.Y()) - R) < 1e-6;
  });
  CHECK(!top.empty());
  if (top.empty()) {
    return;
  }
  const TopoDS_Shape result = rounded(body, top, 1.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The expected material: within the band R - 1 to R, z from 3 to 4, under
  // the torus of radius 1 about the circle of radius R - 1 at z = 4, inside
  // the tab's top face run on 1 upwards.
  TopoDS_Face tab_top;
  for (TopExp_Explorer e(body, TopAbs_FACE); e.More() && tab_top.IsNull(); e.Next()) {
    BRepAdaptor_Surface s(TopoDS::Face(e.Current()));
    if (s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - 3) < 1e-9 &&
        std::abs(std::abs(s.Plane().Axis().Direction().Z()) - 1) < 1e-9) {
      tab_top = TopoDS::Face(e.Current());
    }
  }
  CHECK(!tab_top.IsNull());
  if (tab_top.IsNull()) {
    return;
  }
  const TopoDS_Shape band = BRepAlgoAPI_Cut(
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 3), gp_Dir(0, 0, 1)), R, 1).Shape(),
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 2), gp_Dir(0, 0, 1)), R - 1, 3).Shape()).Shape();
  const TopoDS_Shape under =
    BRepAlgoAPI_Cut(band, BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0, 0, 4), gp_Dir(0, 0, 1)), R - 1, 1).Shape()).Shape();
  const TopoDS_Shape added =
    BRepAlgoAPI_Common(under, BRepPrimAPI_MakePrism(tab_top, gp_Vec(0, 0, 1)).Shape()).Shape();
  const double added_volume = volume_of(added);
  CHECK(added_volume > 0.5 && added_volume < 5);
  CHECK_NEAR_TOL(volume_of(result) - volume_of(body), added_volume, 1e-6);
  const TopoDS_Shape expected = BRepAlgoAPI_Fuse(body, added).Shape();
  CHECK(volume_of(BRepAlgoAPI_Cut(result, expected).Shape()) < 1e-6);
  CHECK(volume_of(BRepAlgoAPI_Cut(expected, result).Shape()) < 1e-6);
}

// A block 11 high whose vertical corner is rounded with 2 about the z axis,
// its faces beside the corner leaving the rounding at 120 and 210 degrees
// (one of them, the slanted face, running off at -60 degrees), lowered to
// 6 high on the side y < w: the step's wall y = w crosses the corner's
// rounding at a sharp vertical edge and stands on the lower top. With `low`
// only the block 6 high.
constexpr double kStepWall = -0.538;

TopoDS_Shape block_stepped_across_its_rounded_corner(bool low) {
  const TopoDS_Shape       box = BRepPrimAPI_MakeBox(gp_Pnt(-2, -2, 0), gp_Pnt(28, 28, low ? 6 : 11)).Shape();
  BRepFilletAPI_MakeFillet corner(box);
  for (const TopoDS_Edge& e : edges_where(box, [](const gp_Pnt& m) {
         return std::abs(m.X() + 2) < 1e-9 && std::abs(m.Y() + 2) < 1e-9;
       })) {
    corner.Add(2, e);
  }
  if (!build(corner)) {
    return TopoDS_Shape();
  }
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), -M_PI / 3);
  const TopoDS_Shape turned = BRepBuilderAPI_Transform(corner.Shape(), turn, true).Shape();
  if (low) {
    return turned;
  }
  ShapeUpgrade_UnifySameDomain unify(
    BRepAlgoAPI_Cut(turned, BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, 6), gp_Pnt(50, kStepWall, 12)).Shape()).Shape(), true,
    true, true);
  unify.Build();
  return unify.Shape();
}

// The lower top's slanted edge rounded with 1, on tangentially round the
// corner up to the step's wall (port patch 1170). The contact on the top
// runs onto the wall's foot before the corner: the analytic piece round
// the corner lies wholly past the wall, as does the slanted piece's start,
// both given up (OCCT capped the slanted piece's prolongation with the
// wall, which it does not meet: "bouchon non ecrit"). The wall now cuts
// them: it grows down to the cut, its edge with the corner's rounding
// prolonged down to where the contact on the rounding meets that edge's
// line, 1 below the top. The rounding takes what the lower block alone
// loses to the rounding of its top edges there, on the side of the wall's
// plane away from the upper part.
void test_rounding_round_a_corner_cut_by_a_wall_standing_across_it() {
  const TopoDS_Shape body = block_stepped_across_its_rounded_corner(false);
  const TopoDS_Shape low  = block_stepped_across_its_rounded_corner(true);
  CHECK(!body.IsNull() && !low.IsNull());
  if (body.IsNull() || low.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // The slanted face's top edge: at z = 6, its middle 10 along the face
  // from where it leaves the rounding at 210 degrees.
  const gp_Pnt mid(2 * std::cos(7 * M_PI / 6) + 10 * 0.5, 2 * std::sin(7 * M_PI / 6) - 10 * std::sqrt(3) / 2, 6);
  const auto   slanted = [&mid](const TopoDS_Shape& shape) {
    return edges_where(shape, [&mid](const gp_Pnt& m) {
      // (On the slanted face's top edge: the line through the middle.)
      const gp_Vec d(0.5, -std::sqrt(3) / 2, 0);
      const gp_Vec v(mid, m);
      return std::abs(m.Z() - 6) < 1e-9 && v.Crossed(d).Magnitude() < 1e-6;
    });
  };
  const std::vector<TopoDS_Edge> low_edge = slanted(low);
  CHECK(low_edge.size() == 1);
  const TopoDS_Shape low_rounded = low_edge.size() == 1 ? rounded(low, low_edge, 1.0) : TopoDS_Shape();
  CHECK(!low_rounded.IsNull());
  if (low_rounded.IsNull()) {
    return;
  }
  const TopoDS_Shape below_wall = BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, -1), gp_Pnt(50, kStepWall, 12)).Shape();
  const TopoDS_Shape removed =
    BRepAlgoAPI_Common(BRepAlgoAPI_Cut(low, low_rounded).Shape(), below_wall).Shape();
  const double removed_volume = volume_of(removed);
  CHECK(removed_volume > 1 && removed_volume < 20);
  const std::vector<TopoDS_Edge> edge = slanted(body);
  CHECK(edge.size() == 1);
  if (edge.size() != 1) {
    return;
  }
  const TopoDS_Shape result = rounded(body, edge, 1.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK_NEAR_TOL(volume_of(body) - volume_of(result), removed_volume, 1e-6);
  const TopoDS_Shape expected = BRepAlgoAPI_Cut(body, removed).Shape();
  CHECK(volume_of(BRepAlgoAPI_Cut(result, expected).Shape()) < 1e-6);
  CHECK(volume_of(BRepAlgoAPI_Cut(expected, result).Shape()) < 1e-6);
}

// Raises each edge's tolerance to its curve's largest deviation from
// its faces through its pcurves, sampled at 400 points, with a margin.
void raise_edge_tolerances_to_their_deviations(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_of);
  BRep_Builder builder;
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces_of.FindKey(i));
    double f3 = 0, l3 = 0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, f3, l3);
    if (BRep_Tool::Degenerated(edge) || curve.IsNull()) {
      continue;
    }
    double worst = 0;
    for (const TopoDS_Shape& f : faces_of.FindFromIndex(i)) {
      const TopoDS_Face& face = TopoDS::Face(f);
      double f2 = 0, l2 = 0;
      const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, f2, l2);
      if (pcurve.IsNull()) {
        continue;
      }
      const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
      for (int q = 0; q <= 400; ++q) {
        const double t = f3 + (l3 - f3) * q / 400.0;
        const gp_Pnt2d uv = pcurve->Value(t);
        worst = std::max(worst, curve->Value(t).Distance(surface->Value(uv.X(), uv.Y())));
      }
    }
    if (worst * 1.1 > BRep_Tool::Tolerance(edge)) {
      builder.UpdateEdge(edge, worst * 1.1);
      for (TopExp_Explorer v(edge, TopAbs_VERTEX); v.More(); v.Next()) {
        if (BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())) < worst * 1.1) {
          builder.UpdateVertex(TopoDS::Vertex(v.Current()), worst * 1.1);
        }
      }
    }
  }
}

// A bar of radius 5 along x (axis y = z = 0, from x = 0 to 14) under a
// riser y < -1.7543, joined to it by a concave arc of radius 3 about
// (1.2457, 7.8808): tangent to the riser's face, but crossing the bar at L
// (y 0.5001, z 4.9749) at about 9 degrees instead of touching it (it runs
// 0.0214 into the bar). The concave arc's edge with the end face x = 0 is
// rounded with 0.3, which leaves a sliver of the bar's surface between L's
// end and that rounding. A null shape when a step fails.
constexpr double kCreaseY = 1.2457, kCreaseZ = 7.8808;

TopoDS_Shape bar_with_a_crease_and_a_rim_rounding() {
  const TopoDS_Shape bar   = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), 5, 14).Shape();
  const TopoDS_Shape riser = BRepPrimAPI_MakeBox(gp_Pnt(0, -10, 0), gp_Pnt(14, kCreaseY - 3, 20)).Shape();
  const TopoDS_Shape fill  = BRepAlgoAPI_Cut(
    BRepPrimAPI_MakeBox(gp_Pnt(0, kCreaseY - 3, 3), gp_Pnt(14, 0.8, kCreaseZ)).Shape(),
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1, kCreaseY, kCreaseZ), gp_Dir(1, 0, 0)), 3, 16).Shape()).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(bar, riser).Shape(), fill).Shape(), true, true,
                                     true);
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  // The concave arc's edge with the end face.
  const std::vector<TopoDS_Edge> rim = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.X()) < 1e-9 && std::abs(std::hypot(m.Y() - kCreaseY, m.Z() - kCreaseZ) - 3) < 1e-6;
  });
  if (rim.size() != 1) {
    return TopoDS_Shape();
  }
  // The rim rounding's own edges come out with tolerances a few per cent
  // under their pcurves' deviations, sampled densely (9.4e-7 against
  // 9.6e-7 mm); the corner splits one, and a check of the piece samples
  // it more densely, so the input's tolerances are raised to what they
  // cover first.
  TopoDS_Shape rimmed = rounded(body, rim, 0.3);
  if (!rimmed.IsNull()) {
    raise_edge_tolerances_to_their_deviations(rimmed);
  }
  return rimmed;
}

void test_rounding_pivots_about_a_crease_over_a_sliver() {
  const TopoDS_Shape body = bar_with_a_crease_and_a_rim_rounding();
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // The end face's edges with the bar (radius 5 at x = 0).
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.X()) < 1e-9 && std::abs(std::hypot(m.Y(), m.Z()) - 5) < 1e-6;
  });
  CHECK(!foot.empty());
  const TopoDS_Shape result = foot.empty() ? TopoDS_Shape() : rounded(body, foot, 0.5);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The crease line L: the bar (radius 5 about the x axis) meets the
  // concave arc (radius 3) at its first point from the end vertex.
  const double d = std::hypot(kCreaseY, kCreaseZ);
  const double a = (25 - 9 + d * d) / (2 * d);
  const double h = std::sqrt(25 - a * a);
  const gp_XY  u(kCreaseY / d, kCreaseZ / d), n(-u.Y(), u.X());
  const gp_XY  l1 = a * u + h * n, l2 = a * u - h * n;
  const gp_XY  L  = l1.X() < l2.X() ? l1 : l2;
  // The pivot corner: a horn torus (both radii the rounding's) about L,
  // centred a radius off the end face, touching the end face along the
  // circle of a radius about L's foot.
  int pivots = 0;
  for (TopExp_Explorer ex(result, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face&  face = TopoDS::Face(ex.Current());
    BRepAdaptor_Surface s(face);
    if (s.GetType() != GeomAbs_Torus || std::abs(s.Torus().MajorRadius() - 0.5) > 1e-6 ||
        std::abs(s.Torus().MinorRadius() - 0.5) > 1e-6) {
      continue;
    }
    ++pivots;
    const gp_Ax1 axis = s.Torus().Axis();
    CHECK(axis.Direction().IsParallel(gp_Dir(1, 0, 0), 1e-9));
    CHECK(axis.Location().Distance(gp_Pnt(0.5, L.X(), L.Y())) < 1e-6);
    double on_end_face = 0;
    for (TopExp_Explorer ee(face, TopAbs_EDGE); ee.More(); ee.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(ee.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      BRepAdaptor_Curve c(edge);
      const gp_Pnt      m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
      if (std::abs(m.X()) > 1e-6) {
        continue;
      }
      for (int q = 0; q <= 20; ++q) {
        const gp_Pnt p = c.Value(c.FirstParameter() + (c.LastParameter() - c.FirstParameter()) * q / 20.0);
        CHECK(std::abs(p.X()) < 1e-6);
        CHECK(std::abs(std::hypot(p.Y() - L.X(), p.Z() - L.Y()) - 0.5) < 1e-6);
      }
      on_end_face += GCPnts_AbscissaPoint::Length(c);
    }
    CHECK(on_end_face > 0.1);
  }
  CHECK(pivots == 1);
  // The rounding only takes material away.
  const double removed = volume_of(body) - volume_of(result);
  CHECK(removed > 1.0 && removed < 1.5);
  CHECK(std::abs(volume_of(BRepAlgoAPI_Common(result, body).Shape()) - volume_of(result)) < 1e-6);
}

} // namespace

void fillet_corner_tests() {
  guarded("test_rounding_vanishing_where_its_faces_come_tangent",
          test_rounding_vanishing_where_its_faces_come_tangent);
  guarded("test_two_roundings_mitred_at_a_short_edge", test_two_roundings_mitred_at_a_short_edge);
  guarded("test_rounding_cut_by_a_cylinder_across_its_seam", test_rounding_cut_by_a_cylinder_across_its_seam);
  guarded("test_rounding_ending_where_the_top_turns_into_a_rim", test_rounding_ending_where_the_top_turns_into_a_rim);
  guarded("test_walk_round_the_corner_past_a_short_edge", test_walk_round_the_corner_past_a_short_edge);
  guarded("test_rounding_cut_by_faces_past_their_bounds", test_rounding_cut_by_faces_past_their_bounds);
  guarded("test_rounding_one_face_up_to_the_hole", test_rounding_one_face_up_to_the_hole);
  guarded("test_walk_round_the_corner_through_a_face_met_at_a_point",
          test_walk_round_the_corner_through_a_face_met_at_a_point);
  guarded("test_rounding_past_the_end_of_a_tapering_strip", test_rounding_past_the_end_of_a_tapering_strip);
  guarded("test_rounding_cut_by_the_face_of_the_edge_it_rolls_on",
          test_rounding_cut_by_the_face_of_the_edge_it_rolls_on);
  guarded("test_rounding_rolled_on_across_a_seam_past_its_end", test_rounding_rolled_on_across_a_seam_past_its_end);
  guarded("test_rounding_rolled_on_past_its_edge_and_cut_by_the_ends",
          test_rounding_rolled_on_past_its_edge_and_cut_by_the_ends);
  guarded("test_rounding_vanishing_where_its_faces_come_tangent_short_of_the_end",
          test_rounding_vanishing_where_its_faces_come_tangent_short_of_the_end);
  guarded("test_rounding_cut_by_a_cylinder_whose_seam_starts_at_the_vertex",
          test_rounding_cut_by_a_cylinder_whose_seam_starts_at_the_vertex);
  guarded("test_rounding_ending_in_the_face_it_runs_into", test_rounding_ending_in_the_face_it_runs_into);
  guarded("test_roundings_roll_before_each_others_faces", test_roundings_roll_before_each_others_faces);
  guarded("test_roundings_crease_over_a_narrow_ring", test_roundings_crease_over_a_narrow_ring);
  guarded("test_rounding_ending_where_its_face_turns_tangentially_into_the_end_face",
          test_rounding_ending_where_its_face_turns_tangentially_into_the_end_face);
  guarded("test_foot_rounding_ending_on_a_rounding_of_its_radius",
          test_foot_rounding_ending_on_a_rounding_of_its_radius);
  guarded("test_rounding_ending_at_a_trimmed_cone_past_its_rim", test_rounding_ending_at_a_trimmed_cone_past_its_rim);
  guarded("test_rounding_running_on_into_an_earlier_roundings_tube",
          test_rounding_running_on_into_an_earlier_roundings_tube);
  guarded("test_rounding_ending_over_pieces_where_the_cap_edge_is_in_two",
          test_rounding_ending_over_pieces_where_the_cap_edge_is_in_two);
  guarded("test_rounding_ending_at_a_block_flush_with_its_contact",
          test_rounding_ending_at_a_block_flush_with_its_contact);
  guarded("test_roundings_joined_across_ribs_with_square_sides", test_roundings_joined_across_ribs_with_square_sides);
  guarded("test_rounding_cut_along_its_side_by_the_face_at_an_onsame_end",
          test_rounding_cut_along_its_side_by_the_face_at_an_onsame_end);
  guarded("test_rounding_rolls_on_the_far_edge_of_a_floor_narrowing_before_the_end",
          test_rounding_rolls_on_the_far_edge_of_a_floor_narrowing_before_the_end);
  guarded("test_rounding_walked_again_past_an_onsame_end_generated_by_its_edge",
          test_rounding_walked_again_past_an_onsame_end_generated_by_its_edge);
  guarded("test_step_foot_rounded_where_a_bar_rounding_tangent_to_the_floor_ends_it",
          test_step_foot_rounded_where_a_bar_rounding_tangent_to_the_floor_ends_it);
  guarded("test_step_foot_on_a_rounding_ended_by_a_rounding_tangent_to_it",
          test_step_foot_on_a_rounding_ended_by_a_rounding_tangent_to_it);
  guarded("test_tab_top_rounded_up_to_its_side_roundings_tangent_to_a_curved_wall",
          test_tab_top_rounded_up_to_its_side_roundings_tangent_to_a_curved_wall);
  guarded("test_rounding_round_a_corner_cut_by_a_wall_standing_across_it",
          test_rounding_round_a_corner_cut_by_a_wall_standing_across_it);
  guarded("test_rounding_pivots_about_a_crease_over_a_sliver", test_rounding_pivots_about_a_crease_over_a_sliver);
}

} // namespace test
