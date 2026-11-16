// SPDX-License-Identifier: MIT
// Corners and ends of roundings built here with OCCT's fillet alone on
// synthetic shapes: the cases behind the patches 0720 and later of the OCCT
// port (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>
#include <functional>
#include <vector>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
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
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The edges of a shape whose middle point passes the test.
std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<TopoDS_Edge>                                      edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge&            edge  = TopoDS::Edge(all(i));
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
    std::fprintf(stderr, "fillet corner case test: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

// The height of the bar below: the rod's trace on the bar's end is an
// ellipse of half axes 10 and 10 / cos(30 degrees), inscribed in the end.
const double kTilt      = kPi / 6.0;
const double kBarHeight = 20.0 / std::cos(kTilt);

// A bar 60 long (x) of section 20 (y) by kBarHeight (z) with a rod of radius
// 10 standing out of its end x = 60, its axis through the middle of the end
// and tilted down by 30 degrees: the rod's trace on the end is inscribed in
// it, touching each side at its middle. The end is left as four pieces, one
// at each corner.
TopoDS_Shape bar_with_tilted_rod() {
  const double       h   = kBarHeight;
  const TopoDS_Shape bar = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(60, 20, h)).Shape();
  const gp_Dir       axis(std::cos(kTilt), 0, -std::sin(kTilt));
  const gp_Pnt       middle(60, 10, h / 2.0);
  // (The rod's seam at its back, below: away from the rounding.)
  const gp_Dir       seam(-0.5, 1.0, -std::cos(kTilt));
  const TopoDS_Shape rod =
    BRepPrimAPI_MakeCylinder(gp_Ax2(middle.Translated(gp_Vec(axis) * -20.0), axis, seam), 10.0, 60.0).Shape();
  const TopoDS_Shape beyond =
    BRepAlgoAPI_Common(rod, BRepPrimAPI_MakeBox(gp_Pnt(60, -100, -100), gp_Pnt(200, 100, 100)).Shape()).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(bar, beyond).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The bar's top front edge rounded with 14 (port patch 0720). The
// rounding's contacts (y = 14 on the top, z = kBarHeight - 14 on the front)
// pass the end's corner piece and reach the end's next pieces beyond the
// rod's trace; the rounding runs on into the rod and is cut by its side.
// The walk round the end vertex meets the rod's side before and after the
// corner piece, across the same edge both times; OCCT cut the rod's side
// twice, through the corner piece, and kept that piece: the result was
// invalid. Now the corner piece, which lies wholly in the removed corner,
// goes: the material removed is the bar's and the rod's within the
// rounding's corner.
void test_rounding_past_an_end_piece_it_consumes() {
  const TopoDS_Shape body = bar_with_tilted_rod();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const double                   h         = kBarHeight;
  const std::vector<TopoDS_Edge> top_front = edges_where(body, [h](const gp_Pnt& m) {
    return m.X() < 60.0 && std::abs(m.Y()) < 1e-7 && std::abs(m.Z() - h) < 1e-7;
  });
  CHECK(top_front.size() == 1);
  if (top_front.size() != 1) {
    return;
  }
  const double       r      = 14.0;
  const TopoDS_Shape result = rounded(body, top_front, r);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The rounding's corner: the region between the edge's faces and the
  // rounding's surface, along the whole bar and the rod.
  const TopoDS_Shape corner = BRepAlgoAPI_Cut(
    BRepPrimAPI_MakeBox(gp_Pnt(-1, -50, h - r), gp_Pnt(200, r, h + 50)).Shape(),
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1, r, h - r), gp_Dir(1, 0, 0)), r, 201.0).Shape())
                                .Shape();
  const double removed = volume_of(BRepAlgoAPI_Common(body, corner).Shape());
  CHECK_NEAR_TOL(volume_of(body) - volume_of(result), removed, 1e-4);
  // No face of the end's corner piece is left.
  int on_corner_piece = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    bool on = true;
    for (TopExp_Explorer v(f.Current(), TopAbs_VERTEX); v.More() && on; v.Next()) {
      const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
      on             = std::abs(p.X() - 60.0) < 1e-6 && p.Y() <= 10.0 + 1e-6 && p.Z() >= h / 2.0 - 1e-6;
    }
    on_corner_piece += on ? 1 : 0;
  }
  CHECK(on_corner_piece == 0);
}

// A post of radius 20 (z 0 to 15) with a rib standing out of its side (x
// 19 to 22, y -0.2 to 2.8, turned by `turn` about the post's axis), the
// post's top edge and the rib's top cut by a cone at 45 degrees (z = 34.5 -
// r, the cone's seam in the plane y = 0, x > 0): the cone's ring over the
// post meets the post's side along the circle r = 20, z = 14.5, and the
// rib's side y = -0.2 meets the cone along an edge from that circle
// outward. Unturned, that side lies just short of the cone's seam.
TopoDS_Shape post_with_rib(double turn, bool spline_post) {
  // (The post's seam at x < 0, away from the rib.)
  const TopoDS_Shape cylinder =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), 20.0, 15.0).Shape();
  const TopoDS_Shape post = spline_post ? BRepBuilderAPI_NurbsConvert(cylinder, true).Shape() : cylinder;
  gp_Trsf rotation;
  rotation.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), turn);
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

// The rounding (0.5) of the rib's side y = -0.2 (turned) with the cone: the
// material it removes and the area of the cone's face after it; negative
// values when OCCT fails or the result is invalid.
struct RibRounding {
  double removed   = -1.0;
  double cone_area = -1.0;
};

RibRounding rib_rounding(double turn, bool spline_post) {
  const TopoDS_Shape body = post_with_rib(turn, spline_post);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> edge = edges_where(body, [turn](const gp_Pnt& m) {
    const gp_Pnt p = m.Rotated(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), -turn);
    const double r = std::hypot(p.X(), p.Y());
    return std::abs(p.Y() + 0.2) < 1e-7 && p.X() > 19.9 && std::abs(p.Z() - (34.5 - r)) < 1e-6;
  });
  CHECK(edge.size() == 1);
  RibRounding out;
  if (edge.size() != 1) {
    return out;
  }
  const TopoDS_Shape result = rounded(body, edge, 0.5);
  if (result.IsNull() || !BRepCheck_Analyzer(result).IsValid()) {
    return out;
  }
  out.removed = volume_of(body) - volume_of(result);
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cone) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(f.Current(), props);
      out.cone_area = props.Mass();
    }
  }
  return out;
}

// The rib's side rounded with 0.5 along its edge with the cone (port patch
// 0721). The rounding starts at the post's side, where the cone's ring and
// the post meet: an end where the cone's edge with the post is prolonged
// past the vertex up to the rounding's contact on the cone. With the rib's
// side just short of the cone's seam, that prolongation crosses the seam:
// the rounding removed less than with the rib turned away from it, the
// cone's face cut wrongly. Now the prolongation is the circle itself, on
// both surfaces, its p-curves parametrized as the circle and split at the
// seam each in the cone's domain; with the post a spline (the edge's curve
// a spline ending at the vertex), the cone and the post are intersected,
// the prolongation's ends taken in one period of the cone (they lay a
// period apart, and the intersection ran the long way round). The cone's
// face is the same in both places, and so is the material removed (on the
// analytic post: on the spline one the volumes of the whole bodies are not
// that exact).
void test_rounding_end_prolonged_across_a_seam() {
  for (const bool spline_post : {false, true}) {
    const RibRounding away   = rib_rounding(1.0, spline_post);
    const RibRounding across = rib_rounding(0.0, spline_post);
    CHECK(away.cone_area > 0.0);
    CHECK(across.cone_area > 0.0);
    CHECK_NEAR_TOL(across.cone_area, away.cone_area, 1e-6);
    if (!spline_post) {
      // 1e-5 mm3: a volume of about 0.15 mm3 measured from two
      // tessellated solids moves by about 1e-6 mm3 when the intersection
      // is split into other pieces (port patch 0480, mitcad#1879), far
      // below the 1e-6 mm length tolerance.
      CHECK_NEAR_TOL(across.removed, away.removed, 1e-5);
    }
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_rounding_past_an_end_piece_it_consumes", test::test_rounding_past_an_end_piece_it_consumes);
  test::guarded("test_rounding_end_prolonged_across_a_seam", test::test_rounding_end_prolonged_across_a_seam);
  if (test::failures == 0) {
    std::puts("test_fillet_corner_cases: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
