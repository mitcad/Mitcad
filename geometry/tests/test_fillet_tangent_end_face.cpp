// SPDX-License-Identifier: MIT
// A rounding that ends where the face at the end goes on tangentially from
// one of its faces, built with OCCT's fillet alone: a pin of radius 4.5
// standing out of a wedge-shaped top face whose sides are tangent to the
// pin, the pin's foot rounded along its arc on the top face. At each end
// the top face's edge with the wedge's side runs into the vertex tangent to
// the arc (a cusp), and the side goes on tangentially from the pin across
// their common edge. The bodies a stored design keeps cut the rounding by
// the side, which grows up to it: the case behind the patch 0911 of the
// OCCT port (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeUpgrade_ShapeDivideClosed.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

constexpr double kPin = 4.5;    // the pin's radius
constexpr double kTop = 10.0;   // the top face's height
constexpr double kAngle = 0.51; // the tangent points' angle from +x, radians

// The wedge's outline: from the tangent points T1 = (r cos a, r sin a) and
// T2 = (r cos a, -r sin a) along the tangents away from +x, closed through
// the pin by the chord between them.
TopoDS_Face wedge_outline(double z) {
  const double c = std::cos(kAngle), s = std::sin(kAngle), far = 40.0;
  const gp_Pnt t1(kPin * c, kPin * s, z), t2(kPin * c, -kPin * s, z);
  BRepBuilderAPI_MakePolygon polygon(t1, gp_Pnt(t1.X() - far * s, t1.Y() + far * c, z),
                                     gp_Pnt(t2.X() - far * s, t2.Y() - far * c, z), t2, true);
  return BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
}

// A block (z 0 to 7) under the wedge (z 7 to 10) and the pin (z 7 to 12)
// standing out of it.
TopoDS_Shape pin_on_a_wedge() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-30, -45, 0), gp_Pnt(10, 45, 7)).Shape();
  const TopoDS_Shape wedge = BRepPrimAPI_MakePrism(wedge_outline(7.0), gp_Vec(0, 0, kTop - 7.0)).Shape();
  const TopoDS_Shape pin =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 7), gp_Dir(0, 0, 1), gp_Dir(std::cos(kAngle), std::sin(kAngle), 0)), kPin, 5.0)
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(block, wedge).Shape(), pin).Shape(), true,
                                     true, true);
  unify.Build();
  return unify.Shape();
}

// The pin's foot on the top face: the circle edge at z = kTop.
TopoDS_Edge foot_of(const TopoDS_Shape& body) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - kPin) < 1e-9 &&
        std::abs(curve.Circle().Location().Z() - kTop) < 1e-9) {
      return TopoDS::Edge(edges(i));
    }
  }
  return TopoDS_Edge();
}

// The material a rounding of radius r adds at the pin's foot all round,
// within the wedge's sides (their planes cut it).
double foot_ring_in_the_wedge(double r) {
  // The section: the square at the corner less the ball's quarter.
  const gp_Pnt c0(kPin, 0, kTop);
  BRepBuilderAPI_MakePolygon square(c0, gp_Pnt(kPin + r, 0, kTop), gp_Pnt(kPin + r, 0, kTop + r),
                                    gp_Pnt(kPin, 0, kTop + r), true);
  const TopoDS_Face corner = BRepBuilderAPI_MakeFace(square.Wire(), true).Face();
  const TopoDS_Shape ball =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kPin + r, -1, kTop + r), gp_Dir(0, 1, 0)), r, 2.0).Shape();
  const TopoDS_Shape section = BRepAlgoAPI_Cut(corner, ball).Shape();
  const TopoDS_Shape ring = BRepPrimAPI_MakeRevol(section, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape();
  const TopoDS_Shape wedge = BRepPrimAPI_MakePrism(wedge_outline(kTop - 1.0), gp_Vec(0, 0, 3.0)).Shape();
  return volume_of(BRepAlgoAPI_Common(ring, wedge).Shape());
}

void test_rounding_cut_by_a_face_going_on_tangentially_from_the_pin() {
  const TopoDS_Shape body = pin_on_a_wedge();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const TopoDS_Edge foot = foot_of(body);
  CHECK(!foot.IsNull());
  if (foot.IsNull()) {
    return;
  }
  const double r = 1.0;
  TopoDS_Shape rounded;
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(r, foot);
    fillet.Build();
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      return;
    }
    rounded = fillet.Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    CHECK(false);
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  // One torus, running on to both of the wedge's sides.
  int tori = 0;
  for (TopExp_Explorer f(rounded, TopAbs_FACE); f.More(); f.Next()) {
    tori += BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Torus ? 1 : 0;
  }
  CHECK(tori == 1);
  // The material added: the ring at the foot within the sides' planes.
  const double added = volume_of(rounded) - volume_of(body);
  CHECK_NEAR_TOL(added, foot_ring_in_the_wedge(r), 1e-6);
}

// A plate (z 7 to 10) whose end is a half round of the pin's radius, its
// sides tangent to it at T1 and T2 and running 7.77 away from +x, and the
// pin (z 10 to 12) standing on its top flush with that end, bored with a
// radius of 3 (mitcad#1336). The pin's cylinder and the plate's end are one
// face, closed round the axis with its seam along the plate's side at T2:
// OCCT's merge of the two (ShapeUpgrade_UnifySameDomain) joins them only
// as open pieces, so the closed pin is cut in two first.
TopoDS_Shape pin_on_a_plate_end() {
  const double c = std::cos(kAngle), s = std::sin(kAngle), side = 7.77, z = 7.0;
  const gp_Pnt t1(kPin * c, kPin * s, z), t2(kPin * c, -kPin * s, z);
  const gp_Pnt a1(t1.X() - side * s, t1.Y() + side * c, z), a2(t2.X() - side * s, t2.Y() - side * c, z);
  BRepBuilderAPI_MakeWire outline;
  outline.Add(BRepBuilderAPI_MakeEdge(t1, a1).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(a1, gp_Pnt(-8, a1.Y(), z)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-8, a1.Y(), z), gp_Pnt(-8, a2.Y(), z)).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-8, a2.Y(), z), a2).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(a2, t2).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(t2, gp_Pnt(kPin, 0, z), t1).Value()).Edge());
  const TopoDS_Shape plate =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire(), true).Face(), gp_Vec(0, 0, kTop - z)).Shape();
  const TopoDS_Shape pin =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kTop), gp_Dir(0, 0, 1), gp_Dir(c, -s, 0)), kPin, 2.0).Shape();
  const TopoDS_Shape bore = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kTop), gp_Dir(0, 0, 1)), 3.0, 3.0).Shape();
  ShapeUpgrade_ShapeDivideClosed divide(BRepAlgoAPI_Cut(BRepAlgoAPI_Fuse(plate, pin).Shape(), bore).Shape());
  divide.SetNbSplitPoints(1);
  divide.Perform();
  ShapeUpgrade_UnifySameDomain unify(divide.Result(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The rounding of the pin's foot on the plate's top: the long arc there.
// It runs round most of the pin, so the other contact's line is closed and
// the blend's parameters at its ends are those of the whole line; at T2
// the plate's side's edge with the pin goes on beyond the vertex as the
// pin's seam. The end past the vertex along a tangent edge (patch 1140)
// built a result that was not valid (two tori, one inside out); the corner
// cuts both ends by the sides as at the wedge above (patch 0911).
void test_rounding_of_a_pin_on_a_plate_end_round_most_of_it() {
  const TopoDS_Shape body = pin_on_a_plate_end();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // (The pin and the plate's end one face.)
  int cylinders = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    cylinders += surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - kPin) < 1e-9 ? 1 : 0;
  }
  CHECK(cylinders == 1);
  TopoDS_Edge foot;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - kPin) < 1e-9 &&
        std::abs(curve.Circle().Location().Z() - kTop) < 1e-9 && curve.LastParameter() - curve.FirstParameter() > 3.2) {
      foot = TopoDS::Edge(edges(i));
    }
  }
  CHECK(!foot.IsNull());
  if (foot.IsNull()) {
    return;
  }
  const double r = 1.0;
  TopoDS_Shape rounded;
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(r, foot);
    fillet.Build();
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      return;
    }
    rounded = fillet.Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    CHECK(false);
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  int tori = 0;
  for (TopExp_Explorer f(rounded, TopAbs_FACE); f.More(); f.Next()) {
    tori += BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Torus ? 1 : 0;
  }
  CHECK(tori == 1);
  // The material added: the ring at the foot within the sides' planes,
  // which the wedge's outline continues.
  CHECK_NEAR_TOL(volume_of(rounded) - volume_of(body), foot_ring_in_the_wedge(r), 1e-6);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_rounding_cut_by_a_face_going_on_tangentially_from_the_pin",
                test::test_rounding_cut_by_a_face_going_on_tangentially_from_the_pin);
  test::guarded("test_rounding_of_a_pin_on_a_plate_end_round_most_of_it",
                test::test_rounding_of_a_pin_on_a_plate_end_round_most_of_it);
  if (test::failures == 0) {
    std::puts("test_fillet_tangent_end_face: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
