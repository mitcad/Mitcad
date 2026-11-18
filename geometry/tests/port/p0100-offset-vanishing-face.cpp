// SPDX-License-Identifier: MIT
// Port patch 0100: BRepOffset_MakeOffset drops a face whose offset vanishes
// within the offset distance (a chamfer strip narrower than what the
// offsets of the faces beside it take from it) and intersects the offsets
// of the faces on both sides of it instead. Before, such a shell failed
// (fewer faces than the body), a closed offset gave no solid, or the
// result was wrong (with the arc join the body itself).
//
// A block 30 x 20 x 10 with its bottom front edge chamfered 0.5 (a strip
// 0.71 wide), shelled inwards from its open top. The strip's offset plane
// passes outside the corner where the offsets of the front and the bottom
// meet from a wall of c / (2 - sqrt 2) = 0.854 on: by 1 the strip vanishes
// (valid, 12 faces, the cavity that of the plain block), by 0.2 it keeps
// its offset, as stock (13 faces, the cavity cut by the strip's offset).
// Both joins. The same with a vertical edge chamfered (the strip running
// up to the open top), and the closed offset inwards. A slab whose top
// face is a C0 B-spline ridge is refused before the vanishing faces are sought
// (BRepOffset_C0Geometry, no exception), as stock: the patch's offset
// surfaces, made without Geom_OffsetSurface's check of C0 continuity, are
// only made of the faces that pass that check.

#include <cmath>
#include <cstdio>

#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_List.hxx>
#include <TopoDS_Face.hxx>

#include "port_test.hpp"

namespace {

constexpr double kW = 30.0, kD = 20.0, kH = 10.0, kC = 0.5;
constexpr double kTol = 1e-6;

// The first solid of a shape (null if none).
TopoDS_Shape solid_of(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    return TopoDS_Shape();
  }
  TopExp_Explorer solids(shape, TopAbs_SOLID);
  return solids.More() ? solids.Current() : TopoDS_Shape();
}

// The block with the edge whose middle is `middle` chamfered kC.
TopoDS_Shape chamfered_block(const gp_Pnt& middle) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(kW, kD, kH).Shape();
  const std::vector<TopoDS_Edge> edges =
      port::edges_where(block, [&middle](const gp_Pnt& p) { return p.Distance(middle) < 1e-6; });
  CHECK(edges.size() == 1u);
  return solid_of(port::chamfered(block, edges, kC));
}

// The face of a shape whose centre of mass lies at z = kH (the top).
TopoDS_Shape top_of(const TopoDS_Shape& shape) {
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(faces.Current(), props);
    if (std::abs(props.CentreOfMass().Z() - kH) < 1e-6) {
      return faces.Current();
    }
  }
  return TopoDS_Shape();
}

// The body shelled inwards by `wall` from its open top; a null shape when
// the offset fails or throws.
TopoDS_Shape shelled(const TopoDS_Shape& body, double wall, GeomAbs_JoinType join) {
  const TopoDS_Shape top = top_of(body);
  CHECK(!top.IsNull());
  NCollection_List<TopoDS_Shape> open;
  open.Append(top);
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(body, open, -wall, kTol, BRepOffset_Skin, false, false, join, false);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "shell: %s\n", failure.what());
    return TopoDS_Shape();
  }
  if (!maker.IsDone()) {
    std::fprintf(stderr, "shell: not done (error %d)\n", static_cast<int>(maker.MakeOffset().Error()));
    return TopoDS_Shape();
  }
  return maker.Shape();
}

// The leg of the triangle the strip's offset plane cuts off the corner of
// the walls' offsets (none where it is not positive).
double leg(double wall) {
  return kC + wall * std::sqrt(2.0) - 2.0 * wall;
}

double cut_off(double wall, double length) {
  const double l = leg(wall);
  return l > 0.0 ? 0.5 * l * l * length : 0.0;
}

// The bottom front edge chamfered, the top open: by 1 the strip vanishes
// and the offsets of the front and the bottom meet; by 0.2 it stays.
void test_chamfer_strip_vanishes_within_the_wall() {
  const TopoDS_Shape body = chamfered_block(gp_Pnt(kW / 2, 0, 0));
  CHECK(port::valid(body));
  if (!port::valid(body)) {
    return;
  }
  const double outer = kW * kD * kH - 0.5 * kC * kC * kW;
  CHECK_NEAR(port::volume_of(body), outer);
  for (const GeomAbs_JoinType join : {GeomAbs_Intersection, GeomAbs_Arc}) {
    for (const double wall : {1.0, 0.2}) {
      const TopoDS_Shape shell = shelled(body, wall, join);
      CHECK(port::valid(shell));
      if (!port::valid(shell)) {
        std::fprintf(stderr, "join %d, wall %g\n", static_cast<int>(join), wall);
        continue;
      }
      const double cavity = (kW - 2 * wall) * (kD - 2 * wall) * (kH - wall) - cut_off(wall, kW - 2 * wall);
      CHECK_NEAR_TOL(port::volume_of(shell), outer - cavity, 1e-7);
      CHECK(port::count_of(shell, TopAbs_FACE) == (wall > 0.854 ? 12 : 13));
    }
  }
}

// A vertical edge chamfered: the strip runs up to the open top, where its
// edge is free; between the two walls it vanishes all along.
void test_vertical_chamfer_strip_vanishes_up_to_the_open_top() {
  const TopoDS_Shape body = chamfered_block(gp_Pnt(kW, 0, kH / 2));
  CHECK(port::valid(body));
  if (!port::valid(body)) {
    return;
  }
  const double outer = kW * kD * kH - 0.5 * kC * kC * kH;
  for (const GeomAbs_JoinType join : {GeomAbs_Intersection, GeomAbs_Arc}) {
    for (const double wall : {1.0, 0.2}) {
      const TopoDS_Shape shell = shelled(body, wall, join);
      CHECK(port::valid(shell));
      if (!port::valid(shell)) {
        std::fprintf(stderr, "join %d, wall %g\n", static_cast<int>(join), wall);
        continue;
      }
      const double cavity = (kW - 2 * wall) * (kD - 2 * wall) * (kH - wall) - cut_off(wall, kH - wall);
      CHECK_NEAR_TOL(port::volume_of(shell), outer - cavity, 1e-7);
      CHECK(port::count_of(shell, TopAbs_FACE) == (wall > 0.854 ? 12 : 13));
    }
  }
}

// The closed offset inwards of the block with its bottom front edge
// chamfered: by 1 the plain block's offset (a solid of six faces), by 0.2
// with the strip's offset (seven).
void test_closed_offset_drops_the_vanishing_strip() {
  const TopoDS_Shape body = chamfered_block(gp_Pnt(kW / 2, 0, 0));
  if (body.IsNull()) {
    return;
  }
  for (const double wall : {1.0, 0.2}) {
    BRepOffsetAPI_MakeOffsetShape maker;
    try {
      OCC_CATCH_SIGNALS
      maker.PerformByJoin(body, -wall, kTol, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "offset %g: %s\n", wall, failure.what());
    }
    CHECK(maker.IsDone());
    if (!maker.IsDone()) {
      continue;
    }
    const TopoDS_Shape inner = maker.Shape();
    CHECK(port::count_of(inner, TopAbs_SOLID) == 1);
    CHECK(port::valid(inner));
    const double expected = (kW - 2 * wall) * (kD - 2 * wall) * (kH - 2 * wall) - cut_off(wall, kW - 2 * wall);
    CHECK_NEAR_TOL(port::volume_of(inner), expected, 1e-7);
    CHECK(port::count_of(inner, TopAbs_FACE) == (wall > 0.854 ? 6 : 7));
  }
}

// A slab whose top and bottom are a ridge, a B-spline of degree 1 with an
// inner knot (C0, not flat, so that the offset does not replace it with a
// plane): the shell open at the front is refused as C0 geometry, without
// an exception, before any face's offset surface is made.
void test_c0_bspline_top_is_refused_as_stock() {
  NCollection_Array2<gp_Pnt> poles(1, 3, 1, 2);
  for (int i = 1; i <= 3; ++i) {
    for (int j = 1; j <= 2; ++j) {
      poles(i, j) = gp_Pnt(kW * (i - 1) / 2.0, kD * (j - 1), i == 2 ? kH + 2.0 : kH);
    }
  }
  NCollection_Array1<double> uKnots(1, 3);
  NCollection_Array1<int>    uMults(1, 3);
  uKnots(1) = 0.0;
  uKnots(2) = 0.5;
  uKnots(3) = 1.0;
  uMults(1) = 2;
  uMults(2) = 1;
  uMults(3) = 2;
  NCollection_Array1<double> vKnots(1, 2);
  NCollection_Array1<int>    vMults(1, 2);
  vKnots(1) = 0.0;
  vKnots(2) = 1.0;
  vMults(1) = 2;
  vMults(2) = 2;
  const occ::handle<Geom_BSplineSurface> surface =
      new Geom_BSplineSurface(poles, uKnots, vKnots, uMults, vMults, 1, 1);
  CHECK(surface->Continuity() == GeomAbs_C0);
  const TopoDS_Face  top   = BRepBuilderAPI_MakeFace(surface, 1e-7).Face();
  const TopoDS_Shape block = solid_of(BRepPrimAPI_MakePrism(top, gp_Vec(0, 0, -kH)).Shape());
  CHECK(!block.IsNull());
  if (block.IsNull()) {
    return;
  }
  CHECK_NEAR(std::abs(port::volume_of(block)), kW * kD * kH);
  TopoDS_Shape front;
  for (TopExp_Explorer faces(block, TopAbs_FACE); faces.More(); faces.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(faces.Current(), props);
    if (std::abs(props.CentreOfMass().Y()) < 1e-6) {
      front = faces.Current();
    }
  }
  CHECK(!front.IsNull());
  NCollection_List<TopoDS_Shape> open;
  open.Append(front);
  BRepOffsetAPI_MakeThickSolid maker;
  bool                         threw = false;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(block, open, -1.0, kTol, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "shell of the C0 top: %s\n", failure.what());
    threw = true;
  }
  CHECK(!threw);
  CHECK(!maker.IsDone());
  CHECK(maker.MakeOffset().Error() == BRepOffset_C0Geometry);
}

} // namespace

MITCAD_PORT_TEST("0100", test_chamfer_strip_vanishes_within_the_wall);
MITCAD_PORT_TEST("0100", test_vertical_chamfer_strip_vanishes_up_to_the_open_top);
MITCAD_PORT_TEST("0100", test_closed_offset_drops_the_vanishing_strip);
MITCAD_PORT_TEST("0100", test_c0_bspline_top_is_refused_as_stock);
