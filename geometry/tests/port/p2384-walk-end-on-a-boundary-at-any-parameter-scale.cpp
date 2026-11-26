// SPDX-License-Identifier: MIT
// Port patch 2384: a walking intersection line's end is put on a surface's
// boundary within a room that grows with a parameter range longer than a
// full turn (IntWalk_PWalking::PutToBoundary), so that a surface
// parametrized by arc length is treated as one parametrized by angle or
// over [0, 1] (mitcad#2384, GitHub mitcad/Mitcad#641).
//
// The body: MitCad's loft from a circle of radius 170 at z = 315 (on the z
// axis) to one of radius 85 at z = 0 centred at x = 225, an oblique cone
// whose side is a B-spline parametrized by arc length (u 0 .. 1068.14,
// v 0 .. 387.10), and its mirror image about x = 0 (the kernel's own
// transform). The two sides cross in the plane x = 0 along a curve from
// (0, 170, 315) down to (0, 0, 142.26) and up to (0, -170, 315), sharing
// their tangent plane at both ends.
//
// - The section of the two sides: its crossing curve ends on both points
//   (within 1e-6 mm). Without the patch the walk stopped 0.004288 mm from
//   (0, -170, 315), 5e-3 in v below the top, where a thousandth of a
//   parameter was the room to put it on the boundary.
// - The kernel's union of the two bodies: one valid solid with the union's
//   volume (every horizontal section of the loft is a disc centred at
//   x >= 0, so the union is twice the loft's half at x >= 0, integrated over
//   the height) and reaching x = -310 and 310. Without the patch the top
//   circle was not split at (0, -170, 315), the mirror image's side was
//   classified inside the loft as a whole, and the union was the loft
//   alone (3 faces, its volume).
// - The kernel's own through-sections loft of the same circles (v over
//   [0, 1], within the room as before): the same union, as before.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/profile.hpp"
#include "port_test.hpp"

namespace {

namespace g = mitcad::geometry;

constexpr double kHeight = 315.0;
constexpr double kTop = 170.0;
constexpr double kBottom = 85.0;
constexpr double kShift = 225.0;

// MitCad's loft (geometry/src/loft_skin.cpp): its side parametrized by arc
// length.
TopoDS_Shape mitcad_loft() {
  const auto section = [](double cx, double z, double radius, const char* name) {
    g::LoftSection s;
    s.frame.origin = gp_Pnt(0, 0, z);
    s.region = g::circle_region(cx, 0, radius);
    s.region.loops[0].segments[0].name = name;
    s.region.name = std::string("r{") + name + "}";
    return s;
  };
  g::LoftSpec spec;
  spec.feature = "F4";
  spec.sections = {section(0.0, kHeight, kTop, "c2"), section(kShift, 0.0, kBottom, "c1")};
  spec.merge_tangent = true;
  return g::loft(spec)->occt();
}

// The kernel's own loft of the same circles (v over [0, 1]).
TopoDS_Shape kernel_loft() {
  BRepOffsetAPI_ThruSections maker(true, false);
  maker.AddWire(BRepBuilderAPI_MakeWire(
                    BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(0, 0, kHeight), gp_Dir(0, 0, 1)), kTop)).Edge())
                    .Wire());
  maker.AddWire(BRepBuilderAPI_MakeWire(
                    BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(kShift, 0, 0), gp_Dir(0, 0, 1)), kBottom)).Edge())
                    .Wire());
  maker.Build();
  return maker.Shape();
}

TopoDS_Shape mirrored(const TopoDS_Shape& shape) {
  gp_Trsf trsf;
  trsf.SetMirror(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)));
  return BRepBuilderAPI_Transform(shape, trsf, true).Shape();
}

TopoDS_Face side_of(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face face = TopoDS::Face(it.Current());
    if (BRepAdaptor_Surface(face, false).GetType() != GeomAbs_Plane) {
      return face;
    }
  }
  return TopoDS_Face();
}

// The union's volume: twice the loft's half at x >= 0 (Simpson's rule).
double union_volume() {
  const auto area = [](double z) {
    const double centre = kShift * (1.0 - z / kHeight);
    const double radius = kBottom + (kTop - kBottom) * z / kHeight;
    const double disc = test::kPi * radius * radius;
    if (centre >= radius) {
      return disc;
    }
    return disc - (radius * radius * std::acos(centre / radius) - centre * std::sqrt(radius * radius - centre * centre));
  };
  const int steps = 100000;
  double sum = area(0.0) + area(kHeight);
  for (int i = 1; i < steps; ++i) {
    sum += (i % 2 == 1 ? 4.0 : 2.0) * area(kHeight * i / steps);
  }
  return 2.0 * sum * kHeight / steps / 3.0;
}

// The section of the sides: the crossing curve's ends nearest to
// (0, 170, 315) and (0, -170, 315), within 1e-6 mm of each.
void check_section(const char* what, const TopoDS_Shape& body, double room = 1e-6) {
  BRepAlgoAPI_Section section(side_of(body), side_of(mirrored(body)), false);
  section.Approximation(true);
  section.Build();
  CHECK(section.IsDone());
  std::vector<gp_Pnt> ends;
  for (TopExp_Explorer it(section.Shape(), TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(it.Current());
    Bnd_Box box;
    BRepBndLib::Add(edge, box);
    double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
    box.Get(x0, y0, z0, x1, y1, z1);
    // The crossing curve leaves the top plane; the top circle's pieces do not.
    if (z0 > kHeight - 1.0) {
      continue;
    }
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(edge, first, last);
    ends.push_back(BRep_Tool::Pnt(first));
    ends.push_back(BRep_Tool::Pnt(last));
  }
  for (const double y : {kTop, -kTop}) {
    const gp_Pnt target(0, y, kHeight);
    double gap = 1e9;
    for (const gp_Pnt& p : ends) {
      gap = std::min(gap, p.Distance(target));
    }
    std::printf("  %s: the crossing curve ends %.3g mm from (0, %g, %g)\n", what, gap, y, kHeight);
    CHECK(gap <= room);
  }
}

// The kernel's union with the mirror image: one valid solid, the union's
// volume, x from -310 to 310.
void check_union(const char* what, const TopoDS_Shape& body) {
  BRepAlgoAPI_Fuse fuse(body, mirrored(body));
  CHECK(fuse.IsDone());
  if (!fuse.IsDone()) {
    return;
  }
  const TopoDS_Shape result = fuse.Shape();
  Bnd_Box box;
  BRepBndLib::AddOptimal(result, box, false, false);
  double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  const double volume = port::volume_of(result);
  std::printf("  %s: %d solids, %d faces, volume %.3f (the union %.3f), x %.3f .. %.3f\n", what,
              port::count_of(result, TopAbs_SOLID), port::count_of(result, TopAbs_FACE), volume, union_volume(), x0,
              x1);
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  CHECK(port::valid(result));
  CHECK(std::abs(volume - union_volume()) <= 1e-6 * union_volume());
  CHECK(std::abs(x0 + kShift + kBottom) <= 1e-3);
  CHECK(std::abs(x1 - kShift - kBottom) <= 1e-3);
}

void test_arc_length_loft_section_reaches_the_tangent_points() {
  check_section("MitCad's loft", mitcad_loft());
}

void test_arc_length_loft_joined_to_its_mirror_image() { check_union("MitCad's loft", mitcad_loft()); }

void test_unit_parameter_loft_unchanged() {
  const TopoDS_Shape body = kernel_loft();
  // The walk's end within the room as before the patch (a thousandth of
  // v over [0, 1]): 4.2e-4 mm on MSVC in 0.74.0-rc1.
  check_section("the kernel's loft", body, 1e-3);
  check_union("the kernel's loft", body);
}

} // namespace

MITCAD_PORT_TEST("2384", test_arc_length_loft_section_reaches_the_tangent_points);
MITCAD_PORT_TEST("2384", test_arc_length_loft_joined_to_its_mirror_image);
MITCAD_PORT_TEST("2384", test_unit_parameter_loft_unchanged);
