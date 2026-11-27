// SPDX-License-Identifier: MIT
// Roundings whose contacts run off a narrow face (the edge overflow of
// mitcad#121), built here with OCCT's fillet alone on synthetic shapes: the
// cases behind the patches 0890 and 0891 of the OCCT port
// (third_party/vcpkg-ports, mitcad#343): a wall's foot whose contact on the
// wall runs off it before the edge's end, which is at a sliver, and a disc's
// rim whose contact on the bottom runs over a hole's rounded mouth.

#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>

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

// The middle of an edge.
gp_Pnt middle_of(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
}

// The edge of the body whose middle is `p`, null where none is.
TopoDS_Edge edge_at(const TopoDS_Shape& body, const gp_Pnt& p) {
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(ex.Current());
    if (!BRep_Tool::Degenerated(edge) && middle_of(edge).Distance(p) < 1e-6) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// The edges rounded with `radius` by OCCT's fillet, null where it fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, const TopoDS_Edge& edge, double radius) {
  if (edge.IsNull()) {
    return TopoDS_Shape();
  }
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(radius, edge);
    fillet.Build();
    if (fillet.IsDone()) {
      return fillet.Shape();
    }
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "end overflow test: %s\n", failure.what());
  }
  return TopoDS_Shape();
}

// The faces of `shape` on the plane z = `z` (within 1e-6).
int faces_on_plane_z(const TopoDS_Shape& shape, double z) {
  int count = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(ex.Current()));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    if (plane.Axis().Direction().IsParallel(gp::DZ(), 1e-9) && std::abs(plane.Location().Z() - z) < 1e-6) {
      ++count;
    }
  }
  return count;
}

// The radius of the wall's rounding and of the earlier rounding of its end.
constexpr double kRadius = 0.5;

// A half-cylinder of radius 7 lying on a floor (the plane y = 0), its axis
// along x, from x = -10 to its flat end x = 0, the end's rim rounded with 1
// (a torus tangent to the end along a circle of radius 6) and the feet on
// the floor rounded with kRadius all round (the feet of the cylinder, of the
// torus and of the end). Then a box x > 0, y > 0, z < `cutEnd` is cut off:
// the end's plane becomes a wall down to the floor, and the feet's rounding
// of the torus is cut by the wall up to the circle where the torus touches
// the wall. Where `cutEnd` lies past the torus's foot the wall's foot edge
// ends at both sides where the wall meets the torus's foot rounding; where
// it lies in that rounding, the box's end face leaves a sliver at the
// wall's foot (bounded by the floor, the wall and the rounding).
TopoDS_Shape body(double cutEnd) {
  const TopoDS_Shape cylinder =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-10, 0, 0), gp::DX()), 7.0, 10.0).Shape();
  const TopoDS_Shape upper = BRepPrimAPI_MakeBox(gp_Pnt(-11, 0, -8), gp_Pnt(1, 8, 8)).Shape();
  const TopoDS_Shape half  = BRepAlgoAPI_Common(cylinder, upper).Shape();
  const TopoDS_Shape slab  = BRepPrimAPI_MakeBox(gp_Pnt(-12, -2, -12), gp_Pnt(6, 0, 12)).Shape();
  ShapeUpgrade_UnifySameDomain fused(BRepAlgoAPI_Fuse(half, slab).Shape());
  fused.Build();
  TopoDS_Shape shape = fused.Shape();
  // The end's rim: the half circle of radius 7 in the plane x = 0.
  shape = rounded(shape, edge_at(shape, gp_Pnt(0, 7, 0)), 1.0);
  if (shape.IsNull()) {
    return shape;
  }
  // The feet: the cylinder's foot at z = 7, with its tangent chain.
  shape = rounded(shape, edge_at(shape, gp_Pnt(-5.5, 0, 7)), kRadius);
  if (shape.IsNull()) {
    return shape;
  }
  const TopoDS_Shape tool = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, -20), gp_Pnt(10, 10, cutEnd)).Shape();
  ShapeUpgrade_UnifySameDomain cut(BRepAlgoAPI_Cut(shape, tool).Shape());
  cut.Build();
  return cut.Shape();
}

// The wall's foot rounded with kRadius. Its contact on the wall leaves the
// wall at both ends across the wall's edge with the feet's rounding, where
// that rounding's ball is the wall's: the end is that rounding's OnSame
// end, which grows up to the section they share (port patch 0540). With
// the sliver at one end, the wall's foot ends at the sliver, not at that
// rounding; it is the same end across the sliver, which is consumed (port
// patch 0890): the result is the one without the sliver with the body's
// material beyond the cut added back. Without the patch the corner found no
// face at the end holding both contacts' ends, went round the vertex, and
// failed.
void test_wall_foot_ending_at_a_sliver() {
  // The sliver: at z = 6.92 the feet's rounding stands 0.02 high at the
  // wall (it reaches the wall's plane at z = 7.118).
  const double       sliverEnd = 6.92;
  const TopoDS_Shape withSliver = body(sliverEnd);
  const TopoDS_Shape without    = body(20.0);
  CHECK(!withSliver.IsNull() && !without.IsNull());
  if (withSliver.IsNull() || without.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(withSliver).IsValid());
  CHECK(BRepCheck_Analyzer(without).IsValid());
  CHECK(faces_on_plane_z(withSliver, sliverEnd) == 1);
  const TopoDS_Shape plain = rounded(without, edge_at(without, gp_Pnt(0, 0, 0)), kRadius);
  CHECK(!plain.IsNull());
  // The wall's foot runs from the far end's rounding to the sliver.
  const TopoDS_Edge foot = [&] {
    for (TopExp_Explorer ex(withSliver, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge edge = TopoDS::Edge(ex.Current());
      const gp_Pnt      m    = middle_of(edge);
      if (std::abs(m.X()) < 1e-6 && std::abs(m.Y()) < 1e-6) {
        return edge;
      }
    }
    return TopoDS_Edge();
  }();
  const TopoDS_Shape result = rounded(withSliver, foot, kRadius);
  CHECK(!result.IsNull());
  if (plain.IsNull() || result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(plain).IsValid());
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The sliver is consumed.
  CHECK(faces_on_plane_z(result, sliverEnd) == 0);
  // The ends are as without the sliver: the result is that one with the
  // body's material beyond the cut.
  const double expected = volume_of(BRepAlgoAPI_Fuse(plain, withSliver).Shape());
  CHECK_NEAR_TOL(volume_of(result), expected, 1e-7);
  // The rounding adds material (a concave edge).
  CHECK(volume_of(result) > volume_of(withSliver));
}


// The disc's rim: the circle of radius 10 on its bottom face (y = 0).
TopoDS_Edge rim_of(const TopoDS_Shape& shape) {
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(ex.Current());
    const gp_Pnt      m    = middle_of(edge);
    if (!BRep_Tool::Degenerated(edge) && std::abs(m.Y()) < 1e-6 && std::abs(std::hypot(m.X(), m.Z()) - 10.0) < 1e-6) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// A disc of radius 10 standing on its bottom face (y = 0 to 3), its faces
// B-splines (so that roundings of its edges are walked, not built in closed
// form), its rim's circle running round from +x (where its vertex is) away
// from +z. With `holed`, a blind hole of radius 1 drilled up into it from
// the bottom, its axis 7.8 from the disc's at `angle` round it (from +x
// towards +z), the hole's mouth rounded with 0.5: the mouth's rounding
// reaches 9.3 from the disc's axis on the bottom face, its seam on the side
// away from the axis.
TopoDS_Shape disc(bool holed, double angle) {
  const TopoDS_Shape plain = BRepBuilderAPI_NurbsConvert(
                               BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 3, 0), -gp::DY(), gp::DX()), 10.0, 3.0).Shape())
                               .Shape();
  if (plain.IsNull() || !holed) {
    return plain;
  }
  const gp_Dir       out(std::cos(angle), 0.0, std::sin(angle));
  const gp_Pnt       foot(7.8 * out.X(), -1.0, 7.8 * out.Z());
  const TopoDS_Shape hole    = BRepPrimAPI_MakeCylinder(gp_Ax2(foot, gp::DY(), out), 1.0, 3.0).Shape();
  const TopoDS_Shape drilled = BRepAlgoAPI_Cut(plain, hole).Shape();
  // The mouth: the hole's circle on the bottom face.
  TopoDS_Edge mouth;
  for (TopExp_Explorer ex(drilled, TopAbs_EDGE); ex.More() && mouth.IsNull(); ex.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(ex.Current());
    const gp_Pnt      m    = middle_of(edge);
    if (!BRep_Tool::Degenerated(edge) && std::abs(m.Y()) < 1e-6
        && std::abs(std::hypot(m.X() - foot.X(), m.Z() - foot.Z()) - 1.0) < 1e-6) {
      mouth = edge;
    }
  }
  return rounded(drilled, mouth, 0.5);
}

// The disc's rim rounded with 1: the ball's contact on the bottom face (a
// circle of radius 9) runs over the hole's mouth, which the ball cannot
// touch (its rounding is sharper than the ball), so the blend is walked on
// over the gap and the mouth's rounding cuts it (port patch 0160), the cut
// crossing that rounding's seam. The result is the disc rounded without
// the hole, less the hole and its mouth: away from the rim's vertex, and
// with the gap across the vertex (the closed spine's start) on either side
// of it. With the gap across the start, the piece over its stretch past
// the start took the vertex where the cut crosses the seam, just before
// that stretch, for a vertex of the mouth's rounding under the blend (its
// nearest point on the contact's line the line's end), and the stripe
// failed without port patch 0891.
void test_disc_rim_over_a_rounded_hole_mouth() {
  const TopoDS_Shape plainDisc = disc(false, 0.0);
  CHECK(!plainDisc.IsNull());
  if (plainDisc.IsNull()) {
    return;
  }
  const TopoDS_Shape plain = rounded(plainDisc, rim_of(plainDisc), 1.0);
  CHECK(!plain.IsNull());
  for (const double angle : {kPi, 0.05, -0.05}) {
    const TopoDS_Shape body = disc(true, angle);
    CHECK(!body.IsNull());
    if (body.IsNull() || plain.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(body).IsValid());
    const TopoDS_Shape result = rounded(body, rim_of(body), 1.0);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    const double expected = volume_of(BRepAlgoAPI_Common(plain, body).Shape());
    CHECK_NEAR_TOL(volume_of(result), expected, 1e-7);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_end_overflow) {
  test::guarded("test_wall_foot_ending_at_a_sliver", test::test_wall_foot_ending_at_a_sliver);
  test::guarded("test_disc_rim_over_a_rounded_hole_mouth", test::test_disc_rim_over_a_rounded_hole_mouth);
  if (test::failures == 0) {
    std::puts("test_fillet_end_overflow: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
