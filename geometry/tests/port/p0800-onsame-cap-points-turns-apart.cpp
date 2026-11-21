// SPDX-License-Identifier: MIT
// Port patch 0800: at an OnSame end whose face at the end lies on a
// periodic surface, ChFi3d_Recale brings the cap's two points on that face
// into one period however many periods apart they lie (the unpatched code
// moved one of them by one period only).
//
// A post of radius 20 with a rib standing out of its side, the post's top
// and the rib's top cut by a cone at 45 degrees; the rib's side rounded
// with 0.5 along its edge with the cone. The rounding starts at the post's
// side (an OnSame end, the post's cylinder the face at the end): the cap's
// point on the arc comes from the arc's curve on the cylinder (pfac on the
// arc's side, u = a), the opposite contact's point from the intersection
// of the cone's contact with the cylinder (p2dbout, IntCurveSurface's
// analytic cylinder parameter, in [0, 2 pi)). The post's cylinder is
// stored trimmed to one period starting `turns` periods below the basis's
// parameters, its edges' curves moved as far, as the bodies stored in files
// hold it: a lies in [-2 pi turns, -2 pi (turns - 1)], the other point at
// p = a + 2 pi turns + d, d the cap's own (small, signed) extent in u.
//
// ChFi3d_Recale without the patch shifts the point not taken for
// reference by one period: for turns = 2 the two points stay a period
// apart, at a and a + 2 pi + d, and the box for the cap's intersection
// (ChFi3d_Boite, then ChFi3d_BoundFac, which for a box of width 2 pi + d on
// a periodic surface moves its ends by -0.1 d) is [a + 0.1 d, a + 2 pi +
// 0.9 d]. With d > 0 the cap [a, a + d] lies (mostly) in it: the rib's side
// y = -0.2 (geometry/tests/test_fillet_crest_corners.cpp,
// test_end_on_a_cylinder_stored_turns_down), whose cone contact lies at a
// larger u than its side's contact, builds either way. The rib's other side,
// y = 2.8, mirrors it: d < 0, the box [a - 0.1 |d|, a + 2 pi - 0.9 |d|]
// holds only a tenth of the cap at either end, and the cap's intersection
// is not found ("OneCorner : echec calcul intersection") or comes out cut
// at the box: the rounding fails or its removed volume and the cone's area
// differ from the plain post's. With the patch the second point is brought
// to a + d.
//
// The boundary: the same side with the cylinder stored one turn down. The
// points are then 2 pi + d apart, and the unpatched code's one-period
// shift already brings them to a and a + d (the patch changes nothing for
// points less than one and a half periods apart): it builds either way, as
// the plain post does.

#include <cmath>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec2d.hxx>

#include "port_test.hpp"

namespace {

constexpr double kTurn = 1.0; // the rib's turn about the post's axis, radians

// Every cylindrical face's surface replaced by its cylinder trimmed to one
// period from the face's start `periods` turns lower, the face's curves on
// it moved down as far (as test_fillet_crest_corners does): the number of
// faces changed.
int cylinders_turns_down(const TopoDS_Shape& body, int periods) {
  const double shift = -2.0 * test::kPi * periods;
  BRep_Builder builder;
  int          changed = 0;
  for (TopExp_Explorer fx(body, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face&        face = TopoDS::Face(fx.Current());
    TopLoc_Location           loc;
    occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, loc);
    if (const occ::handle<Geom_RectangularTrimmedSurface> trimmed =
          occ::down_cast<Geom_RectangularTrimmedSurface>(surface)) {
      surface = trimmed->BasisSurface();
    }
    const occ::handle<Geom_CylindricalSurface> cylinder = occ::down_cast<Geom_CylindricalSurface>(surface);
    if (cylinder.IsNull()) {
      continue;
    }
    ++changed;
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const occ::handle<Geom_RectangularTrimmedSurface> stored =
      new Geom_RectangularTrimmedSurface(cylinder, u0 + shift, u0 + shift + 2.0 * test::kPi, v0 - 10.0, v1 + 10.0);
    std::vector<std::pair<TopoDS_Edge, std::pair<occ::handle<Geom2d_Curve>, occ::handle<Geom2d_Curve>>>> pcurves;
    for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge         edge = TopoDS::Edge(ex.Current());
      double                    f = 0, l = 0;
      occ::handle<Geom2d_Curve> c1 = occ::down_cast<Geom2d_Curve>(BRep_Tool::CurveOnSurface(edge, face, f, l)->Copy());
      occ::handle<Geom2d_Curve> c2;
      c1->Translate(gp_Vec2d(shift, 0.0));
      if (BRep_Tool::IsClosed(edge, face)) {
        c2 = occ::down_cast<Geom2d_Curve>(
          BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Reversed()), face, f, l)->Copy());
        c2->Translate(gp_Vec2d(shift, 0.0));
      }
      pcurves.push_back({edge, {c1, c2}});
    }
    for (const auto& [edge, c] : pcurves) {
      if (c.second.IsNull()) {
        builder.UpdateEdge(edge, c.first, stored, loc, BRep_Tool::Tolerance(edge));
      } else {
        builder.UpdateEdge(edge, c.first, c.second, stored, loc, BRep_Tool::Tolerance(edge));
      }
    }
    builder.UpdateFace(face, stored, loc, BRep_Tool::Tolerance(face));
  }
  return changed;
}

// A post of radius 20 (z 0 to 15, its cylinder's u measured from -x) with a
// rib standing out of its side (x 19 to 22, y -0.2 to 2.8, turned by kTurn
// about the post's axis, at u = pi + 1 on the cylinder, far from its seam),
// the post's top edge and the rib's top cut by a cone at 45 degrees (z =
// 34.5 - distance from the axis).
TopoDS_Shape post_with_rib() {
  const TopoDS_Shape post =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), 20.0, 15.0).Shape();
  gp_Trsf rotation;
  rotation.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), kTurn);
  const TopoDS_Shape rib =
    BRepBuilderAPI_Transform(BRepPrimAPI_MakeBox(gp_Pnt(19, -0.2, 0), gp_Pnt(22, 2.8, 15)).Shape(), rotation, true)
      .Shape();
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(19.5, 0, 15));
  profile.Add(gp_Pnt(30, 0, 4.5));
  profile.Add(gp_Pnt(30, 0, 20));
  profile.Add(gp_Pnt(19.5, 0, 20));
  profile.Close();
  const TopoDS_Shape above = BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(profile.Wire(), true).Face(),
                                                   gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)))
                               .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(BRepAlgoAPI_Fuse(post, rib).Shape(), above).Shape(), true,
                                     true, true);
  unify.Build();
  return unify.Shape();
}

// The rib's side at y (in the rib's own frame) rounded with 0.5 along its
// edge with the cone: the material removed and the cone's area after it;
// negative where OCCT fails or the result is invalid.
struct RibRounding {
  double removed   = -1.0;
  double cone_area = -1.0;
};

RibRounding side_rounding(const TopoDS_Shape& body, double side) {
  const std::vector<TopoDS_Edge> edge = port::edges_where(body, [side](const gp_Pnt& m) {
    const gp_Pnt p = m.Rotated(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), -kTurn);
    const double r = std::hypot(p.X(), p.Y());
    return std::abs(p.Y() - side) < 1e-7 && p.X() > 19.9 && std::abs(p.Z() - (34.5 - r)) < 1e-6;
  });
  CHECK(edge.size() == 1);
  RibRounding out;
  if (edge.size() != 1) {
    return out;
  }
  const TopoDS_Shape result = port::rounded(body, edge, 0.5);
  if (!port::valid(result)) {
    return out;
  }
  out.removed = port::volume_of(body) - port::volume_of(result);
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cone) {
      out.cone_area = port::area_of(f.Current());
    }
  }
  return out;
}

// The side's rounding on the post with its cylinder stored `turns` periods
// down: as on the plain post (the material removed within what the volumes
// integrated over the trimmed cylinder give, the cone's face the same).
void check_side_on_stored_post(double side, int turns) {
  std::printf("the rib's side y = %.1f, the post's cylinder stored %d turns down\n", side, turns);
  const TopoDS_Shape plain  = post_with_rib();
  const TopoDS_Shape stored = post_with_rib();
  CHECK(cylinders_turns_down(stored, turns) == 1);
  CHECK(port::valid(stored));
  CHECK_NEAR_TOL(port::volume_of(stored), port::volume_of(plain), 1e-6);
  const RibRounding on_plain  = side_rounding(plain, side);
  const RibRounding on_stored = side_rounding(stored, side);
  std::printf("  removed %.6f (plain %.6f), cone %.6f (plain %.6f)\n", on_stored.removed, on_plain.removed,
              on_stored.cone_area, on_plain.cone_area);
  CHECK(on_plain.removed > 0.0);
  CHECK(on_plain.cone_area > 0.0);
  CHECK(on_stored.removed > 0.0);
  CHECK_NEAR_TOL(on_stored.cone_area, on_plain.cone_area, 1e-6);
  CHECK_NEAR_TOL(on_stored.removed, on_plain.removed, 2e-3);
}

// The side whose cap runs toward smaller u, the cylinder two turns down:
// the unpatched one-period shift leaves the cap outside the intersection's
// box.
void test_cap_points_two_turns_apart() {
  check_side_on_stored_post(2.8, 2);
}

// The boundary: one turn down, a period and d apart, which the unpatched
// shift already brings together.
void test_cap_points_one_turn_apart() {
  check_side_on_stored_post(2.8, 1);
}

} // namespace

MITCAD_PORT_TEST("0800", test_cap_points_two_turns_apart);
MITCAD_PORT_TEST("0800", test_cap_points_one_turn_apart);
