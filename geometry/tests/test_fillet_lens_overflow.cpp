// SPDX-License-Identifier: MIT
// Roundings built here with OCCT's fillet alone on synthetic shapes: the
// cases behind the patches 0770 and later of the OCCT port
// (third_party/vcpkg-ports, mitcad#343): roundings that roll on an edge
// into a vertex where they vanish, chains of edges through such a vertex,
// two roundings joined across a corner of the solid (one face each),
// the two roundings of a lens joined by an S-shaped section, and a
// rounding rolling on both edges of a lens into its ends.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <functional>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Lin.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The volume of the part of a shape in the box from a to b.
double volume_in(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b) {
  return volume_of(BRepAlgoAPI_Common(shape, BRepPrimAPI_MakeBox(a, b).Shape()).Shape());
}

// The edges of a shape whose middle point passes the test.
std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<TopoDS_Edge>                                      edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(all(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (!curve.IsNull() && keep(curve->Value(0.5 * (first + last)))) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// The edges rounded with radius r by OCCT's fillet alone; a null shape when
// it fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : edges) {
    fillet.Add(r, edge);
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet lens overflow test: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

// Whether the shape has a vertex within tol of p.
bool has_vertex_near(const TopoDS_Shape& shape, const gp_Pnt& p, double tol) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p) < tol) {
      return true;
    }
  }
  return false;
}

// Whether the shape has a vertex at p.
bool has_vertex_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  return has_vertex_near(shape, p, 1e-6);
}

// A block 60 long (x), 20 wide (y) and 20 high (z 50 to 70) on a post of
// radius 10 (z 10 to 50) as wide as the block: the post's top circle
// touches the block's long bottom edges y = +-10 at (0, +-10, 50), where
// the block's underside narrows to nothing, and the post's side there is
// tangent to the block's long sides.
TopoDS_Shape block_on_a_post() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-30, -10, 50), gp_Pnt(30, 10, 70)).Shape();
  const TopoDS_Shape post =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 10), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), 10.0, 40.0).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(block, post).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The post's foot under the block rounded with 10 (the edges of its top
// circle with the block's underside).
TopoDS_Shape post_foot_rounded(const TopoDS_Shape& body) {
  const std::vector<TopoDS_Edge> foot = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 50.0) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - 10.0) < 1e-6;
  });
  CHECK(!foot.empty());
  return foot.empty() ? TopoDS_Shape() : rounded(body, foot, 10.0);
}

// The post's foot rounded with 10 (port patch 0770). The underside's
// contact (20 from the post's axis) runs off the underside across its long
// edges 30 degrees from the x axis; from there the ball rolls on those
// edges, touching the post, up to (0, +-10, 50), where the edges run into
// the post's top tangent to the post's side and the ball has no room left:
// the rounding vanishes there. OCCT took those edges, which end at the
// rounding's end vertex, for no obstacle there and ended each side's
// rounding where the contact left the underside, the corners at that
// vertex filling the rest wrongly: the result was invalid. Now the rounding
// rolls on the edges up to the vertex and ends in a point there, the long
// sides untouched: the result is valid, mirror symmetric, and the material
// added lies between the torus within 30 degrees of the x axis and the
// whole torus round the post.
void test_rounding_rolling_into_a_vertex_where_it_vanishes() {
  const TopoDS_Shape body = block_on_a_post();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const TopoDS_Shape result = post_foot_rounded(body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(has_vertex_at(result, gp_Pnt(0, 10, 50)));
  CHECK(has_vertex_at(result, gp_Pnt(0, -10, 50)));
  // The material added: by quarters, all alike.
  const double big = 100.0;
  double       quarter[4];
  for (int q = 0; q < 4; ++q) {
    const double x0 = (q & 1) != 0 ? 0.0 : -big, x1 = (q & 1) != 0 ? big : 0.0;
    const double y0 = (q & 2) != 0 ? 0.0 : -big, y1 = (q & 2) != 0 ? big : 0.0;
    quarter[q] = volume_in(result, gp_Pnt(x0, y0, 30), gp_Pnt(x1, y1, 50)) -
                 volume_in(body, gp_Pnt(x0, y0, 30), gp_Pnt(x1, y1, 50));
  }
  for (int q = 1; q < 4; ++q) {
    CHECK_NEAR_TOL(quarter[q], quarter[0], 1e-3);
  }
  CHECK_NEAR_TOL(volume_of(result) - volume_of(body), quarter[0] + quarter[1] + quarter[2] + quarter[3], 1e-3);
  // The whole torus adds 2 pi (10 + c) (1 - pi / 4) 100, c the distance of
  // its section's centroid from the corner, (10 - 3 pi) 10 / (12 - 3 pi).
  const double c     = (10.0 - 3.0 * kPi) * 10.0 / (12.0 - 3.0 * kPi);
  const double whole = 2.0 * kPi * (10.0 + c) * (1.0 - kPi / 4.0) * 100.0;
  const double added = volume_of(result) - volume_of(body);
  CHECK(added > whole / 3.0);
  CHECK(added < whole);
}

// After the post's foot is rounded, the block's bottom outline rounded with
// 1 (port patch 0771): a chain of ten edges round the block, along the long
// sides over the foot's rounding, whose faces come tangent at (0, +-10, 50)
// to the long sides (the foot's rounding ends in a point there, its surface
// degenerate at that corner). OCCT carried the chain on through those
// points; its walk crawled into the corner and stopped, or its section
// there took the degenerate face's parameters far from the contact's, and
// no p-curve approximated it. Now the chain stops at each such point, the
// rounding of each side ends in a point there, and the walk's last section
// before it is solved again from the contact's side: the result is valid,
// the points stay, and the four quarters remove the same material.
void test_chain_through_points_where_the_rounding_vanishes() {
  const TopoDS_Shape body = post_foot_rounded(block_on_a_post());
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const std::vector<TopoDS_Edge> outline = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 50.0) < 1e-6
           && (std::abs(std::abs(p.Y()) - 10.0) < 1e-6 || std::abs(std::abs(p.X()) - 30.0) < 1e-6);
  });
  CHECK(outline.size() >= 8);
  const TopoDS_Shape result = rounded(body, outline, 1.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(has_vertex_at(result, gp_Pnt(0, 10, 50)));
  CHECK(has_vertex_at(result, gp_Pnt(0, -10, 50)));
  const double big = 100.0;
  double       quarter[4];
  for (int q = 0; q < 4; ++q) {
    const double x0 = (q & 1) != 0 ? 0.0 : -big, x1 = (q & 1) != 0 ? big : 0.0;
    const double y0 = (q & 2) != 0 ? 0.0 : -big, y1 = (q & 2) != 0 ? big : 0.0;
    quarter[q] = volume_in(body, gp_Pnt(x0, y0, 45), gp_Pnt(x1, y1, 55)) -
                 volume_in(result, gp_Pnt(x0, y0, 45), gp_Pnt(x1, y1, 55));
  }
  CHECK(quarter[0] > 0.0);
  for (int q = 1; q < 4; ++q) {
    CHECK_NEAR_TOL(quarter[q], quarter[0], 1e-3);
  }
}

// The sin of 60 degrees: the housing's second side runs at 60 degrees to its
// first.
const double kSin60 = std::sqrt(3.0) / 2.0;

// The housing's outline (x <= -370 and x / 2 + y sin 60 <= -370, the corner
// between them rounded with 20 about (-390, -225.1666)) as a prism from z0
// to z1.
TopoDS_Shape housing_outline(double z0, double z1) {
  const double             yc = -185.0 / kSin60;
  BRepBuilderAPI_MakePolygon polygon(gp_Pnt(-370, yc, z0), gp_Pnt(-370, -400, z0), gp_Pnt(-560, -400, z0),
                                     gp_Pnt(-560, -90.0 / kSin60, z0), true);
  const TopoDS_Shape prism =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(polygon.Wire()).Shape(), gp_Vec(0, 0, z1 - z0)).Shape();
  const std::vector<TopoDS_Edge> corner = edges_where(prism, [yc](const gp_Pnt& p) {
    return std::abs(p.X() + 370.0) < 1e-6 && std::abs(p.Y() - yc) < 1e-6;
  });
  CHECK(corner.size() == 1);
  return corner.size() == 1 ? rounded(prism, corner, 20.0) : TopoDS_Shape();
}

// The axis of the bulge on the housing's corner: a cylinder of radius
// 49.1354 rising inwards at 19.47 degrees, cut square by its cap 10 along
// the axis from this point.
const gp_Pnt kBulgeAxisPoint(-413.3333, -238.6381, -35.3815);
const gp_Dir kBulgeAxis(-std::sqrt(2.0 / 3.0), -std::sqrt(2.0) / 3.0, 1.0 / 3.0);
const double kBulgeRadius = 49.1354;

// A housing (top z = 0) with a bulge on its rounded corner: the cylinder
// from its cap downwards and outwards within the housing's outline. The
// bulge rises out of the top at the corner, its foot on the top crossing
// the corner's rounding at two points just past its tangent lines, and its
// side up from the rounding is the rounding's surface.
TopoDS_Shape housing_with_a_bulge() {
  const gp_Pnt base = kBulgeAxisPoint.Translated(-200.0 * gp_Vec(kBulgeAxis));
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(gp_Ax2(base, kBulgeAxis), kBulgeRadius, 210.0).Shape();
  const TopoDS_Shape bulge    = BRepAlgoAPI_Common(cylinder, housing_outline(-150.0, 100.0)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(housing_outline(-150.0, 0.0), bulge).Shape(), true, true,
                                     true);
  unify.Build();
  return unify.Shape();
}

// The bulge's foot on the housing's top rounded with 20 (port patch 0772).
// The foot is two edges, from the cap to the corner's rounding on each
// side. The rounding of each rolls on between the top's plane and the
// bulge past the rounding's end: over the corner the ball keeps touching
// the top's plane beyond the housing, its contact leaving the top across
// the first side's edge and coming back across the second's, the bulge
// still beside it. OCCT ended each rounding where its foot edge ended, the
// corners there filling the rest wrongly. Now the two roundings are one
// blend over the corner, cut by the housing's sides, which grow up to it:
// the edges between them run on up to the blend at z = 0.3834. The result
// is valid, the vertices there, nothing beyond the housing's outline above
// its top, and the material added symmetric about the corner's bisector.
//
// With the two edges given in the other order the blend is walked from the
// other stripe, the other way along the spine: the points where the
// contact crosses the sides' edges still keep the edges' parts beyond the
// blend (their curves run the other way relative to the walk).
void test_roundings_joined_across_a_corner(bool other_order) {
  const gp_Pnt       centre(-390.0, -195.0 / kSin60, 0.0);
  const TopoDS_Shape body = housing_with_a_bulge();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const gp_Lin                   axis(kBulgeAxisPoint, kBulgeAxis);
  const std::vector<TopoDS_Edge> foot = edges_where(body, [&axis](const gp_Pnt& p) {
    return std::abs(p.Z()) < 1e-6 && std::abs(axis.Distance(p) - kBulgeRadius) < 1e-4;
  });
  CHECK(foot.size() == 2);
  std::vector<TopoDS_Edge> order = foot;
  if (other_order) {
    std::reverse(order.begin(), order.end());
  }
  const TopoDS_Shape result = rounded(body, order, 20.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The edges between the sides and the corner's rounding (at its tangent
  // lines, 20 from its centre) run on up to the blend, 0.3834 above the top
  // there (to 1e-4).
  CHECK(has_vertex_near(result, gp_Pnt(-370.0, centre.Y(), 0.3834), 1e-4));
  CHECK(has_vertex_near(result, gp_Pnt(-380.0, centre.Y() + 20.0 * kSin60, 0.3834), 1e-4));
  // Nothing beyond the first side above the top.
  CHECK(volume_in(result, gp_Pnt(-369.999, -400.0, 0.0), gp_Pnt(-300.0, -100.0, 60.0)) < 1e-6);
  // The material added on the two sides of the corner's bisector alike:
  // boxes 400 along it, 200 across it (their y towards -across), on either
  // side.
  const gp_Dir along(kSin60, 0.5, 0.0);
  const gp_Vec across(0.5, -kSin60, 0.0);
  double       half[2];
  for (int k = 0; k < 2; ++k) {
    const gp_Ax2 frame(centre.Translated(gp_Vec(along) * -200.0 + across * (k == 0 ? 0.0 : 200.0) + gp_Vec(0, 0, -50.0)),
                       gp_Dir(0, 0, 1), along);
    const TopoDS_Shape side = BRepPrimAPI_MakeBox(frame, 400.0, 200.0, 120.0).Shape();
    half[k] = volume_of(BRepAlgoAPI_Common(result, side).Shape()) - volume_of(BRepAlgoAPI_Common(body, side).Shape());
  }
  CHECK(half[0] > 0.0);
  CHECK_NEAR_TOL(half[1], half[0], 1e-3);
  CHECK_NEAR_TOL(volume_of(result) - volume_of(body), half[0] + half[1], 1e-3);
}

// Whether an edge of a face lies on the plane through p with normal n
// (sampled at its middle and ends), above the housing's top.
bool has_edge_on_plane(const TopoDS_Face& face, const gp_Pnt& p, const gp_Dir& n) {
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      continue;
    }
    bool on = true;
    for (const double t : {0.1, 0.5, 0.9}) {
      const gp_Pnt q = curve->Value(first + t * (last - first));
      on             = on && std::abs(gp_Vec(p, q).Dot(gp_Vec(n))) < 1e-5 && q.Z() > 1e-4;
    }
    if (on) {
      return true;
    }
  }
  return false;
}

// The joined blend of test_roundings_joined_across_a_corner as two faces
// (port patch 0820): one face generated by each foot edge, the first's
// holding the cut by the first side, the second's the cuts by the corner's
// rounding and by the second side, the two meeting along the blend's section
// through the junction on the first side (an edge between them with a vertex
// there). Before, the walk started halfway along the first stripe and made a
// third face: the first edge generated the base piece's face (and the
// walked one was generated by no edge).
void test_joined_roundings_one_face_each(bool other_order) {
  const gp_Pnt       centre(-390.0, -195.0 / kSin60, 0.0);
  const TopoDS_Shape body = housing_with_a_bulge();
  const gp_Lin       axis(kBulgeAxisPoint, kBulgeAxis);
  std::vector<TopoDS_Edge> foot = edges_where(body, [&axis](const gp_Pnt& p) {
    return std::abs(p.Z()) < 1e-6 && std::abs(axis.Distance(p) - kBulgeRadius) < 1e-4;
  });
  CHECK(foot.size() == 2);
  if (foot.size() != 2) {
    return;
  }
  if (other_order) {
    std::reverse(foot.begin(), foot.end());
  }
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : foot) {
    fillet.Add(20.0, edge);
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet lens overflow test: %s\n", failure.what());
  }
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  // The blend's faces each edge generates (the corners at the far ends add
  // none: a face at the cap cuts each blend).
  std::vector<TopoDS_Face> faces[2];
  for (int k = 0; k < 2; ++k) {
    for (const TopoDS_Shape& s : fillet.Generated(foot[static_cast<std::size_t>(k)])) {
      if (s.ShapeType() == TopAbs_FACE) {
        faces[k].push_back(TopoDS::Face(s));
      }
    }
    CHECK(faces[k].size() == 1);
  }
  if (faces[0].size() != 1 || faces[1].size() != 1) {
    return;
  }
  const gp_Dir second(0.5, kSin60, 0.0);
  const gp_Pnt onSecond(-380.0, centre.Y() + 20.0 * kSin60, 0.0);
  const bool   firstIsA = has_edge_on_plane(faces[0].front(), gp_Pnt(-370.0, 0.0, 0.0), gp::DX());
  const TopoDS_Face& a = firstIsA ? faces[0].front() : faces[1].front();
  const TopoDS_Face& b = firstIsA ? faces[1].front() : faces[0].front();
  CHECK(has_edge_on_plane(a, gp_Pnt(-370.0, 0.0, 0.0), gp::DX()));
  CHECK(!has_edge_on_plane(a, onSecond, second));
  CHECK(has_edge_on_plane(b, onSecond, second));
  CHECK(!has_edge_on_plane(b, gp_Pnt(-370.0, 0.0, 0.0), gp::DX()));
  // Their common edge, from the junction on the first side.
  bool shared = false;
  for (TopExp_Explorer ea(a, TopAbs_EDGE); ea.More() && !shared; ea.Next()) {
    for (TopExp_Explorer eb(b, TopAbs_EDGE); eb.More() && !shared; eb.Next()) {
      if (ea.Current().IsSame(eb.Current())) {
        shared = has_vertex_near(ea.Current(), gp_Pnt(-370.0, centre.Y(), 0.3834), 1e-4);
      }
    }
  }
  CHECK(shared);
}

// A shaft of radius 25 (z 0 to 60) with a key along it: a cylinder of
// radius 5.1854 about (0, 20.3525) from z = 21.432 to 41.432, standing out
// of the shaft up to 0.538. The key's end at z = 21.432 is a lens between
// the key's convex arc and the shaft's concave arc (x within +-2.5454).
TopoDS_Shape key_on_a_shaft() {
  const TopoDS_Shape shaft =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), 25.0, 60.0).Shape();
  const TopoDS_Shape key =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 20.3525, 21.432), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)), 5.1854, 20.0)
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(shaft, key).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// Both edges of the key's lens rounded with 2 (port patch 0821): wider
// than the lens, so that the two roundings make one S-shaped section. The
// concave rounding's ball touches the shaft and rolls on the lens's convex
// edge; the convex rounding's ball touches the key and the concave one from
// outside; the lens is consumed, the concave blend's face lies between its
// contact on the shaft and the convex blend's contact on it, and beyond
// those contacts' meeting points the shaft cuts the convex blend up to
// where the convex blend's contact on the key reaches the key's sides (at
// z = 21.584, as in the stored designs' rounding of such a key). OCCT
// rolled each rounding on the other's edge and the concave one's walk
// failed. The result is valid, has no face left on the lens's plane, and
// is mirror symmetric.
void test_lens_rounded_on_both_edges() {
  const TopoDS_Shape body = key_on_a_shaft();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> lens = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 21.432) < 1e-6 && std::abs(p.X()) < 3.0 && p.Y() > 20.0;
  });
  CHECK(lens.size() == 2);
  const TopoDS_Shape result = rounded(body, lens, 2.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The convex blend's contact on the key ends on the key's sides.
  CHECK(has_vertex_near(result, gp_Pnt(2.5454, 24.8701, 21.584), 1e-3));
  CHECK(has_vertex_near(result, gp_Pnt(-2.5454, 24.8701, 21.584), 1e-3));
  // No face on the lens's plane.
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(f.Current()));
    CHECK(!(s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - 21.432) < 1e-6 &&
            std::abs(std::abs(s.Plane().Axis().Direction().Z()) - 1.0) < 1e-9));
  }
  // Material added on both sides of x = 0 alike (within 0.01 mm3: the
  // walks are not exactly symmetric).
  const double left  = volume_in(result, gp_Pnt(-10, 15, 15), gp_Pnt(0, 30, 30)) -
                      volume_in(body, gp_Pnt(-10, 15, 15), gp_Pnt(0, 30, 30));
  const double right = volume_in(result, gp_Pnt(0, 15, 15), gp_Pnt(10, 30, 30)) -
                       volume_in(body, gp_Pnt(0, 15, 15), gp_Pnt(10, 30, 30));
  CHECK(std::abs(left - right) < 0.01);
  // The two blends' faces as large as the stored design's (4.844 and 3.922
  // mm2, within 0.01).
  bool convex = false, concave = false;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f.Current(), props);
    convex  = convex || std::abs(props.Mass() - 4.844) < 0.01;
    concave = concave || std::abs(props.Mass() - 3.922) < 0.01;
  }
  CHECK(convex);
  CHECK(concave);
}

// The key on a shaft whose side is two faces, split at z = zs by a circle:
// the key's sides (x = +-2.5456) are split there too. The key's other end,
// at z = 41.432, is again a lens.
TopoDS_Shape key_on_a_split_shaft(double zs) {
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(0, 0, 0));
  profile.Add(gp_Pnt(25, 0, 0));
  profile.Add(gp_Pnt(25, 0, zs));
  profile.Add(gp_Pnt(25, 0, 60));
  profile.Add(gp_Pnt(0, 0, 60));
  profile.Close();
  const TopoDS_Shape shaft =
    BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(profile.Wire()).Face(), gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)))
      .Shape();
  const TopoDS_Shape key =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 20.3525, 21.432), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)), 5.1854, 20.0)
      .Shape();
  return BRepAlgoAPI_Fuse(shaft, key).Shape();
}

// The key's convex arc at its far end rounded with 2 (port patch 0822),
// wider than the lens, the key's sides split at zs: the ball touches the
// key and rolls on the lens's concave edge; where its contact on the key
// reaches the key's sides (z = 40.6018) it rolls on both edges, past the
// vertex where the side is split, up to the lens's ends, where the edges
// meet and the rounding vanishes in a point. OCCT walked on from the first
// end with the faces and edges of its walk at the other end, ended the
// walk at the split, and its last sections into the lens's end, of no
// width, failed the approximation. The result is valid, has no face left
// on the lens's plane, is mirror symmetric, and its faces rolling on both
// edges past the split (each tail mm2, their contact on the lens's concave
// edge from x = +-xq) end in a point at the lens's ends.
void test_lens_end_rolling_on_both_edges(double zs, double xq, double tail) {
  const TopoDS_Shape body = key_on_a_split_shaft(zs);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> arc = edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 41.432) < 1e-6 && std::abs(std::hypot(p.X(), p.Y() - 20.3525) - 5.1854) < 1e-6;
  });
  CHECK(arc.size() == 1);
  const TopoDS_Shape result = rounded(body, arc, 2.0);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The contact on the key ends on its sides; the ball's contacts on the
  // lens's concave edge where it rolls past the split.
  for (const double s : {-1.0, 1.0}) {
    CHECK(has_vertex_near(result, gp_Pnt(s * 2.5456, 24.8701, 40.6018), 1e-3));
    CHECK(has_vertex_near(result, gp_Pnt(s * xq, std::sqrt(625.0 - xq * xq), 41.432), 1e-3));
  }
  // No face on the lens's plane; a face of the rounding past each split
  // ending in a point at the lens's end.
  int tails = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face&        face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface s(face);
    CHECK(!(s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - 41.432) < 1e-6 &&
            std::abs(std::abs(s.Plane().Axis().Direction().Z()) - 1.0) < 1e-9));
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    if (std::abs(props.Mass() - tail) > 0.01 * tail) {
      continue;
    }
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      const gp_Pnt       p    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
      if (BRep_Tool::Degenerated(edge) && std::abs(std::abs(p.X()) - 2.5456) < 1e-3 &&
          std::abs(p.Z() - 41.432) < 1e-6) {
        ++tails;
      }
    }
  }
  CHECK(tails == 2);
  // Material removed on both sides of x = 0 alike, 0.7787 mm3 in all.
  const double left  = volume_in(body, gp_Pnt(-10, 15, 35), gp_Pnt(0, 30, 45)) -
                      volume_in(result, gp_Pnt(-10, 15, 35), gp_Pnt(0, 30, 45));
  const double right = volume_in(body, gp_Pnt(0, 15, 35), gp_Pnt(10, 30, 45)) -
                       volume_in(result, gp_Pnt(0, 15, 35), gp_Pnt(10, 30, 45));
  CHECK(std::abs(left - right) < 1e-3);
  CHECK(std::abs(left + right - 0.7787) < 0.002);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_lens_overflow) {
  test::guarded("test_rounding_rolling_into_a_vertex_where_it_vanishes",
                test::test_rounding_rolling_into_a_vertex_where_it_vanishes);
  test::guarded("test_chain_through_points_where_the_rounding_vanishes",
                test::test_chain_through_points_where_the_rounding_vanishes);
  test::guarded("test_roundings_joined_across_a_corner",
                [] { test::test_roundings_joined_across_a_corner(false); });
  test::guarded("test_roundings_joined_across_a_corner (other order)",
                [] { test::test_roundings_joined_across_a_corner(true); });
  test::guarded("test_joined_roundings_one_face_each",
                [] { test::test_joined_roundings_one_face_each(false); });
  test::guarded("test_joined_roundings_one_face_each (other order)",
                [] { test::test_joined_roundings_one_face_each(true); });
  test::guarded("test_lens_rounded_on_both_edges", test::test_lens_rounded_on_both_edges);
  test::guarded("test_lens_end_rolling_on_both_edges",
                [] { test::test_lens_end_rolling_on_both_edges(41.0, 2.0976, 0.0963); });
  test::guarded("test_lens_end_rolling_on_both_edges (narrow tails)",
                [] { test::test_lens_end_rolling_on_both_edges(41.3, 2.4087, 0.00907); });
  if (test::failures == 0) {
    std::puts("test_fillet_lens_overflow: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
