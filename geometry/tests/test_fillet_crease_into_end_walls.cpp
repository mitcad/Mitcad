// SPDX-License-Identifier: MIT
// A drafted peg's corner edges rounded down to a block under it, built with
// OCCT's fillet alone. A square peg hangs from a plate, its sides drafted by
// a few degrees; at some depth it runs into a block whose walls go on
// straight down from the peg's sides there (concave creases of the draft's
// angle) and whose vertical corners are rounded with less than the peg's
// rounding. So the peg's sharp corners poke out of the block's rounded
// corners below the creases, and the peg's corner edges end on those
// corners' cylinders. Each corner edge rounded: the ball rolls down the
// corner until its contacts reach the creases, where the block's walls, which
// go on smoothly into the rounded corner at the edge's end vertex, are the
// faces at the end: they and the corner's cylinder cut the blend and grow up
// to the cut, and the poking corner below goes. The result is the peg's
// corners rounded along their whole length together with the block's corner
// cylinders carried on up into them. OCCT took the walls for the sides
// continued (normals within a tenth of a radian): its ball rolled on over
// the creases and on down the block's corners, which it rounded with the
// peg's radius; the port's patch 0980 then made it roll on the creases'
// edges, rolling on both at once, and the fillet failed. A stored design
// whose drafted peg stands on such a block is the case behind the patch 1020
// of the OCCT port (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kHalf      = 5.75; // the peg's half width at the plate (z = 0)
constexpr double kPegBottom = -8.0; // inside the block
constexpr double kBlockTop  = -4.0; // where the peg's sides meet the walls
constexpr double kBlockFoot = -8.5;
constexpr double kCorner    = 0.4; // the block's corners
constexpr double kRadius    = 0.5; // the peg's corner edges

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The peg's half width at height z, its sides drafted by the angle draft.
double half_at(const double z, const double draft) {
  return kHalf + z * std::tan(draft);
}

// The drafted peg from z0 up to z1 (planar faces).
TopoDS_Shape peg(const double z0, const double z1, const double draft) {
  const double a = half_at(z0, draft), b = half_at(z1, draft);
  const gp_Pnt lo[4] = {gp_Pnt(-a, -a, z0), gp_Pnt(a, -a, z0), gp_Pnt(a, a, z0), gp_Pnt(-a, a, z0)};
  const gp_Pnt hi[4] = {gp_Pnt(-b, -b, z1), gp_Pnt(b, -b, z1), gp_Pnt(b, b, z1), gp_Pnt(-b, b, z1)};
  BRepBuilderAPI_Sewing sewing(1e-7);
  sewing.Add(BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakePolygon(lo[0], lo[1], lo[2], lo[3], true).Wire()).Face());
  sewing.Add(BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakePolygon(hi[0], hi[1], hi[2], hi[3], true).Wire()).Face());
  for (int i = 0; i < 4; ++i) {
    const int j = (i + 1) % 4;
    sewing.Add(BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakePolygon(lo[i], lo[j], hi[j], hi[i], true).Wire()).Face());
  }
  sewing.Perform();
  BRepBuilderAPI_MakeSolid solid(TopoDS::Shell(sewing.SewedShape()));
  TopoDS_Solid result = solid.Solid();
  BRepLib::OrientClosedSolid(result);
  return result;
}

// The vertical edges of a shape (lines whose ends differ only in z).
NCollection_List<TopoDS_Shape> vertical_edges(const TopoDS_Shape& shape) {
  NCollection_List<TopoDS_Shape> edges;
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(e)) {
      continue;
    }
    double f = 0., l = 0.;
    const auto c = BRep_Tool::Curve(e, f, l);
    const gp_Pnt a = c->Value(f), b = c->Value(l);
    if (std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9 && std::abs(a.Z() - b.Z()) > 1e-3) {
      edges.Append(e);
    }
  }
  return edges;
}

// The block from z0 to z1, its vertical corners rounded with kCorner.
TopoDS_Shape block(const double z0, const double z1, const double half) {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-half, -half, z0), gp_Pnt(half, half, z1)).Shape();
  BRepFilletAPI_MakeFillet rounded(box);
  for (const TopoDS_Shape& e : vertical_edges(box)) {
    rounded.Add(kCorner, TopoDS::Edge(e));
  }
  rounded.Build();
  return rounded.Shape();
}

TopoDS_Shape plate() {
  return BRepPrimAPI_MakeBox(gp_Pnt(-10., -10., 0.), gp_Pnt(10., 10., 2.)).Shape();
}

// The peg's corner edges in the body: lines from the plate down whose ends
// lie on the peg's corner line.
NCollection_List<TopoDS_Shape> corner_edges(const TopoDS_Shape& body, const double draft) {
  NCollection_List<TopoDS_Shape> edges;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(e)) {
      continue;
    }
    double f = 0., l = 0.;
    const auto c = BRep_Tool::Curve(e, f, l);
    const gp_Pnt a = c->Value(f), b = c->Value(l);
    const auto onCorner = [&](const gp_Pnt& p) {
      const double h = half_at(p.Z(), draft);
      return std::abs(std::abs(p.X()) - h) < 1e-6 && std::abs(std::abs(p.Y()) - h) < 1e-6;
    };
    if (onCorner(a) && onCorner(b) && std::abs(a.Z() - b.Z()) > 1.0) {
      bool known = false;
      for (const TopoDS_Shape& k : edges) {
        known = known || k.IsSame(e);
      }
      if (!known) {
        edges.Append(e);
      }
    }
  }
  return edges;
}

void rounding_case(const double degrees) {
  const double       draft = degrees * kPi / 180.;
  const double       wall  = half_at(kBlockTop, draft);
  const TopoDS_Shape body =
    BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(plate(), peg(kPegBottom, 1., draft)).Shape(),
                     block(kBlockFoot, kBlockTop, wall))
      .Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const NCollection_List<TopoDS_Shape> corners = corner_edges(body, draft);
  CHECK(corners.Extent() == 4);
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Shape& e : corners) {
    fillet.Add(kRadius, TopoDS::Edge(e));
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& error) {
    std::printf("draft %g: %s\n", degrees, error.what());
  }
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    std::printf("draft %g: not done\n", degrees);
    return;
  }
  const TopoDS_Shape result = fillet.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The same rounding built otherwise: the peg on the plate alone, its
  // corners rounded along their whole length, with the block carried on up
  // into it (its rounded corners stand out of the peg's just above the
  // creases). The cut where the block's corner cylinders meet the blend is
  // an approximated intersection of two nearly tangent cylinders (edge
  // tolerances of a few micrometres): the material agrees within 1e-3.
  const TopoDS_Shape       lone = BRepAlgoAPI_Fuse(plate(), peg(kPegBottom, 1., draft)).Shape();
  BRepFilletAPI_MakeFillet alone(lone);
  for (const TopoDS_Shape& e : corner_edges(lone, draft)) {
    alone.Add(kRadius, TopoDS::Edge(e));
  }
  alone.Build();
  CHECK(alone.IsDone());
  if (!alone.IsDone()) {
    return;
  }
  const TopoDS_Shape expected = BRepAlgoAPI_Fuse(alone.Shape(), block(kBlockFoot, -2., wall)).Shape();
  const double       loss     = volume_of(body) - volume_of(result);
  const double       exact    = volume_of(body) - volume_of(expected);
  std::printf("draft %g: material taken %.7f, the peg rounded alone with the block carried up %.7f\n", degrees,
              loss, exact);
  CHECK(std::abs(loss - exact) < 1e-3);
  // The block's corner cylinders stay, from its foot up past the creases
  // (they went where the block's corners were rounded on down).
  int grown = 0;
  for (TopExp_Explorer ex(result, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face&  face = TopoDS::Face(ex.Current());
    BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Cylinder) {
      continue;
    }
    const gp_Cylinder cylinder = surface.Cylinder();
    if (std::abs(cylinder.Axis().Direction().Z()) < 1. - 1e-9) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::AddOptimal(face, box, false, false);
    double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
    box.Get(x0, y0, z0, x1, y1, z1);
    if (std::abs(cylinder.Radius() - kCorner) < 1e-9 && z0 < kBlockFoot + 1e-6 && z1 > kBlockTop + 0.1) {
      ++grown;
    }
  }
  std::printf("draft %g: %d corner cylinders from the foot up past the creases\n", degrees, grown);
  CHECK(grown == 4);
}

// Drafts of 3 to 5 degrees: the creases' kinks lie between the angular
// tolerance and the tenth of a radian within which OCCT takes faces for
// tangent. (With less, the cylinders' cut grazes more and its edge
// tolerances grow to a hundredth of a millimetre.) The corner edges come in
// either way along the corner, so the walk reaches the creases going up from
// the block and going down to it.
void test_drafted_peg_rounded_into_a_block() {
  rounding_case(3.0);
  rounding_case(4.0);
  rounding_case(5.0);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_crease_into_end_walls) {
  test::guarded("test_drafted_peg_rounded_into_a_block", test::test_drafted_peg_rounded_into_a_block);
  if (test::failures == 0) {
    std::puts("test_fillet_crease_into_end_walls: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
