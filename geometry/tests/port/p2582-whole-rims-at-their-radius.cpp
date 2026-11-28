// SPDX-License-Identifier: MIT
// Port patch 2582: whole circular rims rounded with a radius that closes a
// cap to a pole or takes every face of the body away (mitcad#2582, GitHub
// mitcad/Mitcad#741).
//
// The reported cases, with OCCT's fillet alone:
// - a cylinder of radius 5 and height 10, its top rim rounded with 5: a
//   hemisphere about (0, 0, 5) on the cylinder's lower half, the top face
//   gone, the bottom flat (125 pi + 250 / 3 pi);
// - then its bottom rim rounded with 5 too: a ball of 5 (500 / 3 pi);
// - an annulus 40 across with a bore of 20, 10 high, its four rims rounded
//   with 5 at once: a torus of radii 15 and 5 (750 pi^2), no face of the
//   annulus left.
// Before the patch the first gave an invalid solid (the top face kept whole
// beside the hemisphere), the second failed (the cylinder left as a face of
// no area), the third failed (no solid). Second geometries: the cylinder
// with its rim in two half arcs and a boss of radius 5 on a plate (both
// invalid before, or 1e-4 mm3 off), an annulus 30 across with a bore of 10
// (a torus of radii 10 and 5; failed), and three of the annulus's rims, both
// bottom ones and the top's outer one (the bottom outer rounding met by
// another on both its faces; invalid before).
//
// The boundaries, the same before and after: the cylinder's rim and the
// annulus's four rims rounded with 4.99 (a disc of radius 0.01 and rings
// 0.02 wide left), and the annulus's bottom outer rim with both top ones
// rounded with 5 (no rounding met on both its faces).
//
// A rounding of radius r of a convex rim of radius R between a plane and a
// cylinder takes 2 pi (A (R - r) + r^3 / 6) away
// (Pappus), A = r^2 (1 - pi / 4) the section's area, (R - r) + r^3 / (6 A)
// the radius of its centroid; at a bore of radius R it takes
// 2 pi (A (R + r) - r^3 / 6).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Geom_Circle.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

using test::kPi;

// The volume a rounding of radius r takes from a convex rim of radius R
// (outer: the material inside the circle) or from a bore's rim (inner).
double outer_rim_cut(double R, double r) {
  const double a = r * r * (1.0 - kPi / 4.0);
  return 2.0 * kPi * (a * (R - r) + r * r * r / 6.0);
}
double inner_rim_cut(double R, double r) {
  const double a = r * r * (1.0 - kPi / 4.0);
  return 2.0 * kPi * (a * (R + r) - r * r * r / 6.0);
}

// The circular edges of `shape` at height z whose middle point lies at
// `radius` from the z axis.
std::vector<TopoDS_Edge> rims(const TopoDS_Shape& shape, double z, double radius) {
  return port::edges_where(shape, [z, radius](const gp_Pnt& p) {
    return std::abs(p.Z() - z) < 1e-7 && std::abs(std::hypot(p.X(), p.Y()) - radius) < 1e-7;
  });
}

struct Kinds {
  int plane = 0;
  int cylinder = 0;
  int sphere = 0;
  int torus = 0;
  int other = 0;
};

Kinds kinds_of(const TopoDS_Shape& shape) {
  Kinds kinds;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    switch (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType()) {
    case GeomAbs_Plane:
      ++kinds.plane;
      break;
    case GeomAbs_Cylinder:
      ++kinds.cylinder;
      break;
    case GeomAbs_Sphere:
      ++kinds.sphere;
      break;
    case GeomAbs_Torus:
      ++kinds.torus;
      break;
    default:
      ++kinds.other;
      break;
    }
  }
  return kinds;
}

// Every spherical face lies on the sphere about `centre` of radius r, and
// their areas add up to `area`.
bool spheres_on(const TopoDS_Shape& shape, const gp_Pnt& centre, double r, double area) {
  double total = 0.0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() != GeomAbs_Sphere) {
      continue;
    }
    const gp_Sphere sphere = surface.Sphere();
    if (sphere.Location().Distance(centre) > 1e-7 || std::abs(sphere.Radius() - r) > 1e-7) {
      std::fprintf(stderr, "a sphere of radius %.9g about (%.9g, %.9g, %.9g)\n", sphere.Radius(),
                   sphere.Location().X(), sphere.Location().Y(), sphere.Location().Z());
      return false;
    }
    total += port::area_of(f.Current());
  }
  if (!test::near(total, area, 1e-7)) {
    std::fprintf(stderr, "the spheres' area %.9g, not %.9g\n", total, area);
    return false;
  }
  return true;
}

// Every face is a torus about the z axis through (0, 0, z) with radii
// `major` and `minor`.
bool all_on_torus(const TopoDS_Shape& shape, double z, double major, double minor) {
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() != GeomAbs_Torus) {
      std::fprintf(stderr, "a face of type %d\n", static_cast<int>(surface.GetType()));
      return false;
    }
    const gp_Torus torus = surface.Torus();
    if (torus.Location().Distance(gp_Pnt(0, 0, z)) > 1e-7 || !torus.Axis().Direction().IsParallel(gp::DZ(), 1e-9) ||
        std::abs(torus.MajorRadius() - major) > 1e-7 || std::abs(torus.MinorRadius() - minor) > 1e-7) {
      std::fprintf(stderr, "a torus of radii %.9g and %.9g about (%.9g, %.9g, %.9g)\n", torus.MajorRadius(),
                   torus.MinorRadius(), torus.Location().X(), torus.Location().Y(), torus.Location().Z());
      return false;
    }
  }
  return true;
}

// The area of the planar faces at height z.
double planes_at(const TopoDS_Shape& shape, double z) {
  double total = 0.0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(surface.Plane().Location().Z() - z) < 1e-7 &&
        surface.Plane().Axis().Direction().IsParallel(gp::DZ(), 1e-9)) {
      total += port::area_of(f.Current());
    }
  }
  return total;
}

TopoDS_Shape cylinder() { return BRepPrimAPI_MakeCylinder(5.0, 10.0).Shape(); }

// The cylinder extruded from a disc whose circle is two half arcs: its rims
// are two edges each.
TopoDS_Shape cylinder_in_two_arcs() {
  const occ::handle<Geom_Circle> circle = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ()), 5.0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(circle, 0.0, kPi).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(circle, kPi, 2.0 * kPi).Edge());
  const TopoDS_Face disc = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(disc, gp_Vec(0, 0, 10)).Shape();
}

// An annulus `outer` across with a bore `inner` across, 10 high.
TopoDS_Shape annulus(double outer, double inner) {
  const TopoDS_Shape o = BRepPrimAPI_MakeCylinder(outer / 2.0, 10.0).Shape();
  const TopoDS_Shape i = BRepPrimAPI_MakeCylinder(inner / 2.0, 10.0).Shape();
  BRepAlgoAPI_Cut cut(o, i);
  CHECK(cut.IsDone());
  return cut.IsDone() ? cut.Shape() : TopoDS_Shape();
}

// The annulus's rims at both ends, the outer and the inner.
std::vector<TopoDS_Edge> annulus_rims(const TopoDS_Shape& ring, double outer, double inner) {
  std::vector<TopoDS_Edge> all;
  for (const double z : {0.0, 10.0}) {
    for (const double radius : {outer / 2.0, inner / 2.0}) {
      const std::vector<TopoDS_Edge> some = rims(ring, z, radius);
      CHECK(some.size() == 1);
      all.insert(all.end(), some.begin(), some.end());
    }
  }
  return all;
}

void test_cylinder_rim_closed_to_a_pole() {
  const TopoDS_Shape body = cylinder();
  const std::vector<TopoDS_Edge> top = rims(body, 10.0, 5.0);
  CHECK(top.size() == 1);
  const TopoDS_Shape shape = port::rounded(body, top, 5.0);
  CHECK(port::valid(shape));
  if (shape.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(shape), 250.0 * kPi - outer_rim_cut(5.0, 5.0), 1e-8);
  CHECK_NEAR_TOL(port::volume_of(shape), (125.0 + 250.0 / 3.0) * kPi, 1e-8);
  const Kinds kinds = kinds_of(shape);
  CHECK(kinds.plane == 1 && kinds.cylinder == 1 && kinds.sphere == 1 && kinds.torus == 0 && kinds.other == 0);
  // The hemisphere, tangent to the cylinder's lower half, and the flat
  // bottom; no face at the top.
  CHECK(spheres_on(shape, gp_Pnt(0, 0, 5), 5.0, 50.0 * kPi));
  CHECK_NEAR_TOL(planes_at(shape, 0.0), 25.0 * kPi, 1e-8);
  CHECK(planes_at(shape, 10.0) == 0.0);
  CHECK_NEAR_TOL(port::area_of(shape), (25.0 + 50.0 + 50.0) * kPi, 1e-8);
}

void test_both_rims_a_ball() {
  const TopoDS_Shape body = cylinder();
  const TopoDS_Shape dome = port::rounded(body, rims(body, 10.0, 5.0), 5.0);
  CHECK(!dome.IsNull());
  if (dome.IsNull()) {
    return;
  }
  const std::vector<TopoDS_Edge> bottom = rims(dome, 0.0, 5.0);
  CHECK(bottom.size() == 1);
  const TopoDS_Shape shape = port::rounded(dome, bottom, 5.0);
  CHECK(port::valid(shape));
  if (shape.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(shape), 500.0 / 3.0 * kPi, 1e-8);
  const Kinds kinds = kinds_of(shape);
  CHECK(kinds.sphere == 2 && kinds.plane == 0 && kinds.cylinder == 0 && kinds.other == 0);
  CHECK(spheres_on(shape, gp_Pnt(0, 0, 5), 5.0, 100.0 * kPi));
}

void test_rim_in_two_arcs() {
  const TopoDS_Shape body = cylinder_in_two_arcs();
  const std::vector<TopoDS_Edge> top = rims(body, 10.0, 5.0);
  CHECK(top.size() == 2);
  const TopoDS_Shape shape = port::rounded(body, top, 5.0);
  CHECK(port::valid(shape));
  if (shape.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(shape), (125.0 + 250.0 / 3.0) * kPi, 1e-8);
  CHECK(spheres_on(shape, gp_Pnt(0, 0, 5), 5.0, 50.0 * kPi));
  CHECK(planes_at(shape, 10.0) == 0.0);
}

void test_boss_on_a_plate() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, -4), 20.0, 20.0, 4.0).Shape();
  BRepAlgoAPI_Fuse fuse(plate, cylinder());
  CHECK(fuse.IsDone());
  if (!fuse.IsDone()) {
    return;
  }
  ShapeUpgrade_UnifySameDomain unify(fuse.Shape());
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  const std::vector<TopoDS_Edge> top = rims(body, 10.0, 5.0);
  CHECK(top.size() == 1);
  const TopoDS_Shape shape = port::rounded(body, top, 5.0);
  CHECK(port::valid(shape));
  if (shape.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(shape), 1600.0 + (125.0 + 250.0 / 3.0) * kPi, 1e-8);
  CHECK(spheres_on(shape, gp_Pnt(0, 0, 5), 5.0, 50.0 * kPi));
  CHECK(planes_at(shape, 10.0) == 0.0);
}

void test_annulus_four_rims_a_torus() {
  for (const double outer : {40.0, 30.0}) {
    const double inner = outer - 20.0;
    const TopoDS_Shape ring = annulus(outer, inner);
    if (ring.IsNull()) {
      return;
    }
    const TopoDS_Shape shape = port::rounded(ring, annulus_rims(ring, outer, inner), 5.0);
    CHECK(port::valid(shape));
    if (shape.IsNull()) {
      std::fprintf(stderr, "the annulus %g across: no result\n", outer);
      continue;
    }
    const double major = (outer + inner) / 4.0;
    // A whole torus: 2 pi^2 R r^2, its area 4 pi^2 R r.
    CHECK_NEAR_TOL(port::volume_of(shape), 2.0 * kPi * kPi * major * 25.0, 1e-8);
    CHECK_NEAR_TOL(port::area_of(shape), 4.0 * kPi * kPi * major * 5.0, 1e-8);
    CHECK(port::count_of(shape, TopAbs_SOLID) == 1);
    CHECK(port::count_of(shape, TopAbs_FACE) == 4);
    CHECK(all_on_torus(shape, 5.0, major, 5.0));
  }
}

// Three of the annulus's four rims: both bottom rims and the top's outer
// one. The bottom's outer rounding meets the bottom's inner one on the
// bottom and the top's outer one on the outer wall, both faces taken away:
// its contacts on both sides are lines of other roundings (no face on
// either side), and it came out reversed (an invalid solid of 3206.59 mm3).
void test_annulus_rounding_ridden_on_both_sides() {
  const TopoDS_Shape ring = annulus(40.0, 20.0);
  if (ring.IsNull()) {
    return;
  }
  const std::vector<TopoDS_Edge> all = annulus_rims(ring, 40.0, 20.0);
  const std::vector<TopoDS_Edge> three = {all[0], all[1], all[2]};
  const TopoDS_Shape shape = port::rounded(ring, three, 5.0);
  CHECK(port::valid(shape));
  if (shape.IsNull()) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(shape), 3000.0 * kPi - 2.0 * outer_rim_cut(20.0, 5.0) - inner_rim_cut(10.0, 5.0),
                 1e-8);
  CHECK(port::count_of(shape, TopAbs_FACE) == 5);
  // (Left: the top between the rims, the top inner rim, the bore.)
  CHECK(port::count_of(shape, TopAbs_SOLID) == 1);
}

// The boundaries: built the same with and without the patch.
void test_near_and_part_roundings_unchanged() {
  {
    const TopoDS_Shape body = cylinder();
    const TopoDS_Shape shape = port::rounded(body, rims(body, 10.0, 5.0), 4.99);
    CHECK(port::valid(shape));
    if (!shape.IsNull()) {
      CHECK_NEAR_TOL(port::volume_of(shape), 250.0 * kPi - outer_rim_cut(5.0, 4.99), 1e-8);
      // The top left as a disc of radius 0.01.
      CHECK_NEAR_TOL(planes_at(shape, 10.0), kPi * 0.01 * 0.01, 1e-6);
    }
  }
  const TopoDS_Shape ring = annulus(40.0, 20.0);
  if (ring.IsNull()) {
    return;
  }
  const std::vector<TopoDS_Edge> all = annulus_rims(ring, 40.0, 20.0);
  {
    const TopoDS_Shape shape = port::rounded(ring, all, 4.99);
    CHECK(port::valid(shape));
    if (!shape.IsNull()) {
      CHECK_NEAR_TOL(port::volume_of(shape),
                     3000.0 * kPi - 2.0 * outer_rim_cut(20.0, 4.99) - 2.0 * inner_rim_cut(10.0, 4.99), 1e-8);
      CHECK(port::count_of(shape, TopAbs_FACE) == 8);
    }
  }
  {
    // The bottom's outer rim and both top rims (all.front() is the bottom's
    // outer rim, then its inner one, then the top's).
    const std::vector<TopoDS_Edge> three = {all[0], all[2], all[3]};
    const TopoDS_Shape shape = port::rounded(ring, three, 5.0);
    CHECK(port::valid(shape));
    if (!shape.IsNull()) {
      CHECK_NEAR_TOL(port::volume_of(shape),
                     3000.0 * kPi - 2.0 * outer_rim_cut(20.0, 5.0) - inner_rim_cut(10.0, 5.0), 1e-8);
      CHECK(port::count_of(shape, TopAbs_FACE) == 5);
    }
  }
}

} // namespace

MITCAD_PORT_TEST("2582", test_cylinder_rim_closed_to_a_pole);
MITCAD_PORT_TEST("2582", test_both_rims_a_ball);
MITCAD_PORT_TEST("2582", test_rim_in_two_arcs);
MITCAD_PORT_TEST("2582", test_boss_on_a_plate);
MITCAD_PORT_TEST("2582", test_annulus_four_rims_a_torus);
MITCAD_PORT_TEST("2582", test_annulus_rounding_ridden_on_both_sides);
MITCAD_PORT_TEST("2582", test_near_and_part_roundings_unchanged);
