// SPDX-License-Identifier: MIT
// Two concave edges rounded at a convex corner whose own edge is left
// sharp: a wall with a slot, its floor flush with the top of a ledge in
// front of the wall. The slot's floor edge along its side wall runs into
// the wall's front face, and the ledge's edge along the wall's front face
// runs from the ledge's side to the slot's side wall; the slot's side wall
// and the wall's front face meet in a sharp convex edge. Each rounding runs
// on past the corner by its radius until it meets the other (the two
// quarter cylinders cut by each other).
//
// The rounding of the slot's floor edge runs on over the ledge for the
// corner, and the ledge's edge lies within a few radii of its contact on
// the floor, on the side its blend covers. The OCCT port took the ledge's
// edge for one under that blend's ball (patch 0700's stripes under another
// one's ball) and dropped its rounding, although the ledge's edge lies
// beyond the slot's side wall, where the blend fills nothing: the slot's
// rounding ended at the wall's front face and the ledge's edge stayed
// sharp. Patch 1060 of the OCCT port (third_party/vcpkg-ports,
// mitcad#840). The stored design behind it rounds such corners on both
// sides of a slot with 1 mm.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kR = 1.0;      // the rounding's radius
constexpr double kSlot = 10.0;  // the slot floor's edge: x from 0 to the wall's front at 10
constexpr double kLedge = 2.0;  // the ledge's edge along the wall: y from 3 to the slot's side at 5

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props, 1e-9, true);
  return props.Mass();
}

// The wall x in [0, 10], y in [0, 20], z in [0, 10] with a slot y in
// [5, 15] down to z = 5 across it, and a ledge x in [10, 20], y in [3, 17]
// up to z = 5 in front of it: the slot's floor and the ledge's top are one
// face.
TopoDS_Shape body() {
  const TopoDS_Shape wall = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(10, 20, 10)).Shape();
  const TopoDS_Shape slot = BRepPrimAPI_MakeBox(gp_Pnt(-1, 5, 5), gp_Pnt(11, 15, 11)).Shape();
  const TopoDS_Shape ledge = BRepPrimAPI_MakeBox(gp_Pnt(10, 3, 0), gp_Pnt(20, 17, 5)).Shape();
  const TopoDS_Shape slotted = BRepAlgoAPI_Cut(wall, slot).Shape();
  const TopoDS_Shape fused = BRepAlgoAPI_Fuse(slotted, ledge).Shape();
  ShapeUpgrade_UnifySameDomain unify(fused, true, true, false);
  unify.Build();
  return unify.Shape();
}

// The straight edge of `shape` from a to b (either way), or a null edge.
TopoDS_Edge edge_between(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b) {
  for (TopExp_Explorer x(shape, TopAbs_EDGE); x.More(); x.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(x.Current()));
    const gp_Pnt p = c.Value(c.FirstParameter()), q = c.Value(c.LastParameter());
    if ((p.Distance(a) < 1e-7 && q.Distance(b) < 1e-7) || (p.Distance(b) < 1e-7 && q.Distance(a) < 1e-7)) {
      return TopoDS::Edge(x.Current());
    }
  }
  return TopoDS_Edge();
}

// The material a concave rounding of a right-angled edge adds per length.
double section() { return (1.0 - kPi / 4.0) * kR * kR; }

// The material the two roundings add: each one's section along its edge up
// to the corner, and past the corner the part both quarter cylinders'
// sections fill (2 (5/6 - pi/4) r^3).
double material_wanted() { return section() * (kSlot + kLedge) + (5.0 / 3.0 - kPi / 2.0) * kR * kR * kR; }

// A rounding's face: its quarter cylinder along its edge up to the corner,
// and past it the part up to the other cylinder ((pi/2 - 1) r^2).
double face_wanted(double length) { return kPi / 2.0 * kR * length + (kPi / 2.0 - 1.0) * kR * kR; }

void test_roundings_run_past_the_corner_with_occt_alone() {
  const TopoDS_Shape input = body();
  const TopoDS_Edge slot_edge = edge_between(input, gp_Pnt(0, 5, 5), gp_Pnt(10, 5, 5));
  const TopoDS_Edge ledge_edge = edge_between(input, gp_Pnt(10, 3, 5), gp_Pnt(10, 5, 5));
  CHECK(!slot_edge.IsNull());
  CHECK(!ledge_edge.IsNull());
  if (slot_edge.IsNull() || ledge_edge.IsNull()) {
    return;
  }
  BRepFilletAPI_MakeFillet maker(input);
  maker.Add(kR, slot_edge);
  maker.Add(kR, ledge_edge);
  maker.Build();
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(maker.Shape()).IsValid());
  const double added = volume_of(maker.Shape()) - volume_of(input);
  std::printf("material added %.7f, %.7f wanted\n", added, material_wanted());
  CHECK(std::abs(added - material_wanted()) < 1e-6);
  // Each rounding is one face that runs on past the corner by the radius.
  const struct {
    const TopoDS_Edge* edge;
    double length;
    double x_max, y_max;
  } roundings[] = {{&slot_edge, kSlot, 10.0 + kR, 5.0 + kR}, {&ledge_edge, kLedge, 10.0 + kR, 5.0 + kR}};
  for (const auto& rounding : roundings) {
    const NCollection_List<TopoDS_Shape>& faces = maker.Generated(*rounding.edge);
    CHECK(faces.Extent() == 1);
    if (faces.Extent() != 1) {
      continue;
    }
    const double area = area_of(faces.First());
    std::printf("rounding's face %.7f, %.7f wanted\n", area, face_wanted(rounding.length));
    CHECK(std::abs(area - face_wanted(rounding.length)) < 1e-6);
    Bnd_Box box;
    BRepBndLib::AddOptimal(faces.First(), box, false, false);
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    box.Get(x0, y0, z0, x1, y1, z1);
    CHECK(std::abs(x1 - rounding.x_max) < 1e-6);
    CHECK(std::abs(y1 - rounding.y_max) < 1e-6);
  }
}

void test_roundings_run_past_the_corner_by_name() {
  const TopoDS_Shape input = body();
  const ShapePtr named = import_body(input, "F1", 1);
  const TopoDS_Edge slot_edge = edge_between(named->occt(), gp_Pnt(0, 5, 5), gp_Pnt(10, 5, 5));
  const TopoDS_Edge ledge_edge = edge_between(named->occt(), gp_Pnt(10, 3, 5), gp_Pnt(10, 5, 5));
  std::string slot_name, ledge_name;
  for (int i = 0; i < named->edge_count(); ++i) {
    if (named->edge(i).IsSame(slot_edge)) {
      slot_name = named->edge_name(i);
    }
    if (named->edge(i).IsSame(ledge_edge)) {
      ledge_name = named->edge_name(i);
    }
  }
  CHECK(!slot_name.empty());
  CHECK(!ledge_name.empty());
  if (slot_name.empty() || ledge_name.empty()) {
    return;
  }
  const ShapePtr rounded = fillet("F2", *named, {slot_name, ledge_name}, kR);
  CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
  CHECK(std::abs(volume_of(rounded->occt()) - volume_of(input) - material_wanted()) < 1e-6);
  const std::vector<std::string> rounded_edges = {slot_name, ledge_name};
  for (const std::string& rounded_edge : rounded_edges) {
    int count = 0;
    for (int i = 0; i < rounded->face_count(); ++i) {
      for (const std::string& name : rounded->face_names(i)) {
        count += name == "F2:fillet(" + rounded_edge + ")" ? 1 : 0;
      }
    }
    CHECK(count == 1);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_corner_past_wall) {
  test::guarded("test_roundings_run_past_the_corner_with_occt_alone",
                test::test_roundings_run_past_the_corner_with_occt_alone);
  test::guarded("test_roundings_run_past_the_corner_by_name", test::test_roundings_run_past_the_corner_by_name);
  if (test::failures == 0) {
    std::puts("test_fillet_corner_past_wall: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
