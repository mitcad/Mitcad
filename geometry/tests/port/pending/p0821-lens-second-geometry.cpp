// SPDX-License-Identifier: MIT
// Port patch 0821: both edges of a lens rounded, a face of two edges, one
// concave and one convex, narrower than the roundings: the concave stripe
// fails, and the patch joins the two roundings in one S-shaped section
// (DeferLensRoundings, JoinLensPair), the lens consumed. A second geometry
// beside the key on a shaft of geometry/tests/test_fillet_lens_overflow.cpp
// (a shaft of radius 25, a key of radius 5.19 standing out of it by 0.54,
// rounded with 2).
//
// Here: a vertical cylinder of radius 10 (z from 0 to 30) and a cylinder of
// radius 6 whose axis lies 5 from its axis (from z = 10 up to the top), so
// that the smaller one stands out of the larger by 1. Its end at z = 10
// is a lens between its convex arc (radius 6) and the larger cylinder's
// concave arc (radius 10), 1 wide in the middle and 9.1 long, its ends
// where the two circles cross (x = 8.9, y = +-4.56). The arcs are far more
// curved against the lens's width than the key's. Both of the lens's
// edges rounded with 1.5, wider than the lens: without the patch the
// concave rounding finds no start and the fillet fails. With it: valid,
// one solid, no face left on the lens's plane, the convex blend's contact
// on the smaller cylinder ending on its sides above the lens (where the
// larger cylinder cuts that blend), mirror symmetric.
//
// The boundary: the lens's edges rounded with 0.3, narrower than the lens
// in its middle: where OCCT builds that fillet, it is valid and the lens
// keeps a face on its plane.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kShaft  = 10.0; // the larger cylinder's radius
constexpr double kKey    = 6.0;  // the smaller one's
constexpr double kOffset = 5.0;  // between their axes, along x
constexpr double kLens   = 10.0; // the lens's plane

// Where the two circles cross (x, +-y): the lens's ends and the smaller
// cylinder's sides.
double cross_x() { return (kShaft * kShaft - kKey * kKey + kOffset * kOffset) / (2.0 * kOffset); }
double cross_y() { return std::sqrt(kShaft * kShaft - cross_x() * cross_x()); }

// The two cylinders, their seams turned away from the lens (a seam through
// the lens would split its concave edge).
TopoDS_Shape lens_body() {
  const TopoDS_Shape shaft =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), kShaft, 30.0).Shape();
  const TopoDS_Shape key =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kOffset, 0, kLens), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), kKey,
                             30.0 - kLens)
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(shaft, key).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The lens's two edges: on its plane, outside the larger cylinder's axis
// by more than the crossing.
std::vector<TopoDS_Edge> lens_edges(const TopoDS_Shape& body) {
  return port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - kLens) < 1e-6 && p.X() > cross_x() - 0.1;
  });
}

// Whether a planar face lies on the lens's plane.
bool has_face_on_lens_plane(const TopoDS_Shape& shape) {
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(f.Current()));
    if (s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - kLens) < 1e-6 &&
        std::abs(std::abs(s.Plane().Axis().Direction().Z()) - 1.0) < 1e-9) {
      return true;
    }
  }
  return false;
}

// Whether a vertex of the shape lies on the vertical line through (x, y),
// above the lens's plane by more than 1e-4 and by at most `up`.
bool has_vertex_on_side_above_lens(const TopoDS_Shape& shape, double x, double y, double up) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
    if (std::hypot(p.X() - x, p.Y() - y) < 1e-3 && p.Z() > kLens + 1e-4 && p.Z() <= kLens + up) {
      return true;
    }
  }
  return false;
}

// The volume of the part of a shape in the box from a to b.
double volume_in(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b) {
  return port::volume_of(BRepAlgoAPI_Common(shape, BRepPrimAPI_MakeBox(a, b).Shape()).Shape());
}

// Both of the lens's edges rounded with 1.5, wider than the lens: one
// S-shaped section, the lens consumed.
void test_lens_of_a_cylinder_standing_out_of_another_rounded_on_both_edges() {
  const TopoDS_Shape body = lens_body();
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> lens = lens_edges(body);
  CHECK(lens.size() == 2);
  if (!port::valid(body) || lens.size() != 2) {
    return;
  }
  const TopoDS_Shape result = port::rounded(body, lens, 1.5);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  CHECK(!has_face_on_lens_plane(result));
  // The convex blend's contact on the smaller cylinder ends on its sides,
  // above the lens and below the reach of the rounding.
  CHECK(has_vertex_on_side_above_lens(result, cross_x(), cross_y(), 1.5));
  CHECK(has_vertex_on_side_above_lens(result, cross_x(), -cross_y(), 1.5));
  // Mirror symmetric about y = 0: the material changed alike on both sides
  // (within 0.01: the walks are not exactly symmetric), and less than the
  // two roundings' sections along the lens's arcs (each at most 9.1 x 1.6
  // long) would change it.
  const gp_Pnt lo(0, -15, 0), hi(15, 15, 20), mid_lo(0, 0, 0), mid_hi(15, 0, 20);
  const double left  = volume_in(result, lo, mid_hi) - volume_in(body, lo, mid_hi);
  const double right = volume_in(result, mid_lo, hi) - volume_in(body, mid_lo, hi);
  CHECK(std::abs(left - right) < 0.01);
  CHECK(std::abs(left + right) < 2.0 * test::fillet_loss(1.5) * 2.0 * 9.1 * 1.6);
}

// The boundary: the lens's edges rounded with 0.3, narrower than the lens
// in its middle (1 wide). Where OCCT builds that fillet the lens keeps a
// face on its plane between the two roundings.
void test_lens_wider_than_the_roundings_keeps_its_face() {
  const TopoDS_Shape             body = lens_body();
  const std::vector<TopoDS_Edge> lens = lens_edges(body);
  CHECK(lens.size() == 2);
  if (lens.size() != 2) {
    return;
  }
  const TopoDS_Shape result = port::rounded(body, lens, 0.3);
  if (result.IsNull()) {
    std::printf("p0821 boundary: the rounding with 0.3 is not built\n");
    return;
  }
  CHECK(port::valid(result));
  CHECK(has_face_on_lens_plane(result));
}

} // namespace

MITCAD_PORT_TEST("0821", test_lens_of_a_cylinder_standing_out_of_another_rounded_on_both_edges);
MITCAD_PORT_TEST("0821", test_lens_wider_than_the_roundings_keeps_its_face);
