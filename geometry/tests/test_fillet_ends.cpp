// SPDX-License-Identifier: MIT
// Roundings that OCCT's fillet (TKFillet) failed on at their ends, at the
// seams of closed roundings and at corner caps, built here with OCCT alone
// on synthetic shapes: the cases behind the patches 0024 and later of the
// OCCT port (third_party/vcpkg-ports, mitcad#133).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"

namespace test {
namespace {

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// Builds the operation; false when OCCT throws or does not finish.
bool build(BRepFilletAPI_LocalOperation& op) {
  try {
    OCC_CATCH_SIGNALS
    op.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet end test: %s\n", failure.what());
    return false;
  }
  return op.IsDone();
}

// A cylinder of radius 17 and height 25 whose top has six slots 3 deep,
// 2.8 wide, from radius 12 to 16, at 30 + 60 k degrees: the contact circle
// of a rounding of the top rim crosses each slot, and the cylinder's seam
// (at 0 degrees) lies between two of them.
TopoDS_Shape slotted_cylinder() {
  TopoDS_Shape body = BRepPrimAPI_MakeCylinder(17.0, 25.0).Shape();
  for (int k = 0; k < 6; ++k) {
    const TopoDS_Shape slot = BRepPrimAPI_MakeBox(gp_Pnt(12.0, -1.4, 22.0), gp_Pnt(16.0, 1.4, 26.0)).Shape();
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), (30.0 + 60.0 * k) * kPi / 180.0);
    body = BRepAlgoAPI_Cut(body, BRepBuilderAPI_Transform(slot, turn).Shape()).Shape();
  }
  return body;
}

// The top rim of the slotted cylinder rounded with a radius of 1.5 (patch
// 0024). The torus is cut into seven pieces by the slots, the one that closes
// the circle second in SplitKPart's list; the sort of the pieces repeated it
// and lost others, and the stripe threw "ChFiDS_CommonPoint::Vector".
void test_closed_rounding_across_slots() {
  const TopoDS_Shape body = slotted_cylinder();
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    if (std::abs(a.Z() - 25) < 1e-9 && std::abs(std::hypot(a.X(), a.Y()) - 17) < 1e-9 &&
        TopExp::FirstVertex(edge).IsSame(TopExp::LastVertex(edge))) {
      fillet.Add(1.5, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  // The whole rim would lose 2 pi (17 - c) r^2 (1 - pi / 4), c the distance
  // of the section's centroid from the corner; the slots take a little of it.
  const double r = 1.5;
  const double c = r * (10 - 3 * kPi) / (12 - 3 * kPi);
  const double whole = 2 * kPi * (17 - c) * r * r * (1 - kPi / 4);
  const double removed = volume_of(body) - volume_of(fillet.Shape());
  CHECK(removed > 0.8 * whole && removed < whole);
}

// A regular hexagon of circumradius r at height z, a corner on the x axis.
TopoDS_Wire hexagon(double r, double z) {
  BRepBuilderAPI_MakePolygon polygon;
  for (int k = 0; k < 6; ++k) {
    const double a = k * kPi / 3.0;
    polygon.Add(gp_Pnt(r * std::cos(a), r * std::sin(a), z));
  }
  polygon.Close();
  return polygon.Wire();
}

// A knob: a hexagonal frustum from circumradius 4.6188 at z = 1 to 9.2376
// at z = 4 on a hexagonal prism of circumradius 4.6188 from z = 0, the
// frustum's corners cut by a cylinder of radius 8.5 from z = 3.52 up, under
// a cylinder of radius 8.5 from z = 4 to 6 that overhangs the middle of its
// top edges. The cylinder's faces are one face (its seam passes a corner),
// the overhangs flat slivers at z = 4.
TopoDS_Shape hexagonal_knob() {
  const double bottom = 4.6188, top = 9.2376;
  BRepBuilderAPI_Sewing sewing;
  sewing.Add(BRepBuilderAPI_MakeFace(hexagon(bottom, 1.0), true).Face());
  sewing.Add(BRepBuilderAPI_MakeFace(hexagon(top, 4.0), true).Face());
  for (int k = 0; k < 6; ++k) {
    const double a = k * kPi / 3.0, b = (k + 1) * kPi / 3.0;
    BRepBuilderAPI_MakePolygon side;
    side.Add(gp_Pnt(bottom * std::cos(a), bottom * std::sin(a), 1.0));
    side.Add(gp_Pnt(bottom * std::cos(b), bottom * std::sin(b), 1.0));
    side.Add(gp_Pnt(top * std::cos(b), top * std::sin(b), 4.0));
    side.Add(gp_Pnt(top * std::cos(a), top * std::sin(a), 4.0));
    side.Close();
    sewing.Add(BRepBuilderAPI_MakeFace(side.Wire(), true).Face());
  }
  sewing.Perform();
  TopoDS_Shape frustum = BRepBuilderAPI_MakeSolid(TopoDS::Shell(sewing.SewedShape())).Solid();
  if (volume_of(frustum) < 0) {
    frustum.Reverse();
  }
  const TopoDS_Shape lower = BRepAlgoAPI_Common(frustum, BRepPrimAPI_MakeCylinder(8.5, 4.0).Shape()).Shape();
  const TopoDS_Shape upper =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 4), gp_Dir(0, 0, 1)), 8.5, 2.0).Shape();
  const TopoDS_Shape prism =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(hexagon(bottom, 0.0), true).Face(), gp_Vec(0, 0, 1)).Shape();
  ShapeUpgrade_UnifySameDomain unify(
      BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(lower, upper).Shape(), prism).Shape());
  unify.Build();
  return unify.Shape();
}

// The six slanted corner edges of the knob rounded with a radius of 6
// (patch 0025). The contact lines reach the slivers at z = 4 beyond the
// corner, so each rounding ends against a sliver, the cylinder and the next
// sliver (OCCT's intersection at end). OCCT took the other sliver's crossing
// of the shared circle (an exception), counted the cylinder's seam at one
// corner as an edge there, and left the cylinder's corner patch, the edges
// to the corner and the seam uncut.
void test_rounding_ending_in_a_cylinder_corner() {
  const TopoDS_Shape body = hexagonal_knob();
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const double za = BRep_Tool::Pnt(TopExp::FirstVertex(edge)).Z();
    const double zb = BRep_Tool::Pnt(TopExp::LastVertex(edge)).Z();
    if (std::abs(std::min(za, zb) - 1.0) < 1e-6 && std::max(za, zb) > 3.0 && std::max(za, zb) < 4.0) {
      fillet.Add(6.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 6);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  // The six roundings took 10.4076 mm^3 in the design this comes from.
  CHECK_NEAR_TOL(volume_of(body) - volume_of(fillet.Shape()), 10.4076, 1e-3);
}

// A slab 40 long, 4 thick (y from 0 to 4) and 15 high, its ends rounded
// with a radius of 1, with a window from z = 4 to 11 through it. At each end
// of the window its walls leave the slab's faces tangentially: an arc of
// radius 2 turning 0.75 rad from each face, then a flat across the slab.
TopoDS_Shape slab_with_window() {
  TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(-20, 0, 0), gp_Pnt(20, 4, 15)).Shape();
  BRepFilletAPI_MakeFillet rounded(slab);
  for (TopExp_Explorer e(slab, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9 && rounded.Contour(edge) == 0) {
      rounded.Add(1.0, edge);
    }
  }
  rounded.Build();
  slab = rounded.Shape();
  // The window's outline at z = 4: the ends at x = -10 and 10, reaching past
  // the slab's faces where they do not bound the window.
  const double r = 2.0, turn = 0.75, z = 4.0;
  const double s = std::sin(turn), c = std::cos(turn), sh = std::sin(turn / 2), ch = std::cos(turn / 2);
  BRepBuilderAPI_MakeWire wire;
  const auto line = [&wire](const gp_Pnt& p, const gp_Pnt& q) { wire.Add(BRepBuilderAPI_MakeEdge(p, q).Edge()); };
  const auto arc = [&wire](const gp_Pnt& p, const gp_Pnt& m, const gp_Pnt& q) {
    wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p, m, q).Value()).Edge());
  };
  for (int side = -1; side <= 1; side += 2) {
    const double x0 = side * 10.0, dx = -side;
    const double ya = side < 0 ? 0.0 : 4.0, yb = 4.0 - ya, ys = side < 0 ? 1.0 : -1.0;
    const double ca = ya + ys * r, cb = yb - ys * r;
    line(gp_Pnt(x0, ya - ys, z), gp_Pnt(x0, ya, z));
    arc(gp_Pnt(x0, ya, z), gp_Pnt(x0 + dx * r * sh, ca - ys * r * ch, z), gp_Pnt(x0 + dx * r * s, ca - ys * r * c, z));
    line(gp_Pnt(x0 + dx * r * s, ca - ys * r * c, z), gp_Pnt(x0 + dx * r * s, cb + ys * r * c, z));
    arc(gp_Pnt(x0 + dx * r * s, cb + ys * r * c, z), gp_Pnt(x0 + dx * r * sh, cb + ys * r * ch, z), gp_Pnt(x0, yb, z));
    line(gp_Pnt(x0, yb, z), gp_Pnt(x0, yb + ys, z));
    line(gp_Pnt(x0, yb + ys, z), gp_Pnt(-x0, yb + ys, z));
  }
  const TopoDS_Shape tool =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(wire.Wire()).Face(), gp_Vec(0, 0, 7)).Shape();
  return BRepAlgoAPI_Cut(slab, tool).Shape();
}

// The same solid with its faces in the reverse order. OCCT's fillet takes
// the first face of an edge in the shape's order as the rounding's first
// side: here the slab's faces become the second side.
TopoDS_Shape with_faces_reversed(const TopoDS_Shape& solid) {
  std::vector<TopoDS_Shape> faces;
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back(f.Current());
  }
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (auto f = faces.rbegin(); f != faces.rend(); ++f) {
    builder.Add(shell, *f);
  }
  shell.Closed(true);
  TopoDS_Solid result;
  builder.MakeSolid(result);
  builder.Add(result, shell);
  return result;
}

// The window's top and bottom edges on both faces of the slab rounded with
// a radius of 1 (patch 0026). Each rounding ends where its face of the slab
// runs on tangentially into the window's end wall (the OnSame state), with
// the contact on that face off any edge: OCCT's intersection at end gave up
// on such an end. Now the end wall's arc, the flat and the ceiling are cut
// by the rounding, and the edge between the face and the arc is prolonged
// to the contact. With the faces reversed the slab's face is the rounding's
// second side.
void rounding_ending_where_its_face_turns_into_a_wall(bool faces_reversed) {
  const TopoDS_Shape body = faces_reversed ? with_faces_reversed(slab_with_window()) : slab_with_window();
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    const bool on_face = (std::abs(a.Y()) < 1e-9 && std::abs(b.Y()) < 1e-9) ||
                         (std::abs(a.Y() - 4) < 1e-9 && std::abs(b.Y() - 4) < 1e-9);
    const bool at_window = (std::abs(a.Z() - 4) < 1e-9 && std::abs(b.Z() - 4) < 1e-9) ||
                           (std::abs(a.Z() - 11) < 1e-9 && std::abs(b.Z() - 11) < 1e-9);
    if (on_face && at_window && std::abs(a.X()) < 10 + 1e-9 && std::abs(b.X()) < 10 + 1e-9 &&
        fillet.Contour(edge) == 0) {
      fillet.Add(1.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 4);
  CHECK(build(fillet));
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  // Four roundings 20 long would take 80 (1 - pi / 4); the end walls cut a
  // little off each end.
  const double straight = 80 * (1 - kPi / 4);
  const double removed = volume_of(body) - volume_of(fillet.Shape());
  CHECK(removed > 0.85 * straight && removed < straight);
}

void test_rounding_ending_where_its_face_turns_into_a_wall() {
  rounding_ending_where_its_face_turns_into_a_wall(false);
}

void test_rounding_ending_where_its_second_face_turns_into_a_wall() {
  rounding_ending_where_its_face_turns_into_a_wall(true);
}

} // namespace

void fillet_end_tests() {
  guarded("test_closed_rounding_across_slots", test_closed_rounding_across_slots);
  guarded("test_rounding_ending_in_a_cylinder_corner", test_rounding_ending_in_a_cylinder_corner);
  guarded("test_rounding_ending_where_its_face_turns_into_a_wall",
          test_rounding_ending_where_its_face_turns_into_a_wall);
  guarded("test_rounding_ending_where_its_second_face_turns_into_a_wall",
          test_rounding_ending_where_its_second_face_turns_into_a_wall);
}

} // namespace test
