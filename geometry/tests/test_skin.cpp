// SPDX-License-Identifier: MIT
// Curve and surface fitting (P2): interpolation with end derivatives,
// vector fields in a curve's basis, polynomial forms of rational curves and
// Gordon surfaces through curve networks.

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomConvert.hxx>
#include <Geom_Circle.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Ax2.hxx>

#include "../src/skin.hpp"
#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;
using detail::EndDerivatives;

bool near_point(const gp_Pnt& a, const gp_Pnt& b, double tolerance = 1e-9) {
  if (a.Distance(b) <= tolerance) {
    return true;
  }
  std::fprintf(stderr, "(%.12g, %.12g, %.12g) != (%.12g, %.12g, %.12g)\n", a.X(), a.Y(), a.Z(), b.X(), b.Y(),
               b.Z());
  return false;
}

bool near_vec(const gp_Vec& a, const gp_Vec& b, double tolerance = 1e-9) {
  return near_point(gp_Pnt(a.XYZ()), gp_Pnt(b.XYZ()), tolerance);
}

// A whole circle as a non-periodic B-spline starting at angle `seam`.
occ::handle<Geom_BSplineCurve> circle_curve(const gp_Pnt& center, double radius, double seam = 0.0) {
  const occ::handle<Geom_Circle> circle = new Geom_Circle(gp_Ax2(center, gp_Dir(0, 0, 1)), radius);
  occ::handle<Geom_BSplineCurve> curve =
      GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(circle, seam, seam + 2 * kPi));
  if (curve->IsPeriodic()) {
    curve->SetNotPeriodic();
  }
  return curve;
}

occ::handle<Geom_BSplineCurve> line_curve(const gp_Pnt& a, const gp_Pnt& b) {
  NCollection_Array1<gp_Pnt> poles(1, 2);
  poles(1) = a;
  poles(2) = b;
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = a.Distance(b);
  NCollection_Array1<int> mults(1, 2);
  mults(1) = mults(2) = 2;
  return new Geom_BSplineCurve(poles, knots, mults, 1);
}

using Curves = std::vector<occ::handle<Geom_Curve>>;

Curves curves(std::initializer_list<occ::handle<Geom_Curve>> list) { return Curves(list); }

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-8);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props, 1.0e-8);
  return props.Mass();
}

// The faces sewn into a solid with its material inside.
TopoDS_Shape sewn_solid(const std::vector<TopoDS_Face>& faces) {
  BRepBuilderAPI_Sewing sewing(1.0e-6);
  for (const TopoDS_Face& face : faces) {
    sewing.Add(face);
  }
  sewing.Perform();
  TopoDS_Shell shell;
  for (TopExp_Explorer s(sewing.SewedShape(), TopAbs_SHELL); s.More(); s.Next()) {
    shell = TopoDS::Shell(s.Current());
  }
  if (shell.IsNull()) {
    throw std::runtime_error("no shell was sewn");
  }
  const TopoDS_Shape solid = BRepBuilderAPI_MakeSolid(shell).Solid();
  return solid;
}

void test_interpolation() {
  // A cubic through four points with its end tangents is reproduced.
  const auto cubic = [](double t) { return gp_Pnt(t, t * t - 2 * t, 0.5 * t * t * t - t); };
  const auto cubic_d1 = [](double t) { return gp_Vec(1, 2 * t - 2, 1.5 * t * t - 1); };
  const std::vector<double> params{0.0, 1.0, 2.5, 4.0};
  std::vector<gp_Pnt> points;
  for (const double t : params) {
    points.push_back(cubic(t));
  }
  EndDerivatives start;
  start.first = cubic_d1(0);
  EndDerivatives end;
  end.first = cubic_d1(4);
  const occ::handle<Geom_BSplineCurve> curve = detail::interpolate_with_ends(points, params, start, end);
  CHECK(curve->Degree() == 3 && curve->NbPoles() == 6);
  for (const double t : {0.3, 1.7, 3.2, 4.0}) {
    CHECK(near_point(curve->Value(t), cubic(t)));
  }
  CHECK(near_vec(curve->DN(0.0, 1), cubic_d1(0)) && near_vec(curve->DN(4.0, 1), cubic_d1(4)));

  // Second derivatives at both ends: degree 5, a quintic reproduced.
  const auto quintic = [](double t) { return gp_Pnt(t, std::pow(t, 5) / 100, t * t); };
  const auto quintic_d1 = [](double t) { return gp_Vec(1, std::pow(t, 4) / 20, 2 * t); };
  const auto quintic_d2 = [](double t) { return gp_Vec(0, std::pow(t, 3) / 5, 2); };
  const std::vector<double> few{0.0, 1.5, 3.0};
  EndDerivatives start2;
  start2.first = quintic_d1(0);
  start2.second = quintic_d2(0);
  EndDerivatives end2;
  end2.first = quintic_d1(3);
  end2.second = quintic_d2(3);
  const occ::handle<Geom_BSplineCurve> fifth = detail::interpolate_with_ends(
      {quintic(0), quintic(1.5), quintic(3)}, few, start2, end2);
  CHECK(fifth->Degree() == 5);
  for (const double t : {0.4, 2.2}) {
    CHECK(near_point(fifth->Value(t), quintic(t)));
  }
  CHECK(near_vec(fifth->DN(3.0, 2), quintic_d2(3), 1e-8));

  // Fewer conditions, lower degree: two points and a start tangent make
  // a parabola (the radius of a Direction loft, 20 - v^2 / 90).
  EndDerivatives up;
  up.first = gp_Vec(0, 0, 1);
  const occ::handle<Geom_BSplineCurve> parabola =
      detail::interpolate_with_ends({gp_Pnt(20, 0, 0), gp_Pnt(10, 0, 30)}, {0.0, 30.0}, up, {});
  CHECK(parabola->Degree() == 2);
  CHECK(near_point(parabola->Value(15), gp_Pnt(20 - 225.0 / 90, 0, 15)));

  // The basis depends only on the parameters and the numbers of
  // derivatives; close parameters still give a regular system.
  const detail::EndInterpolation basis({0.0, 0.001, 50.0}, 2, 2);
  CHECK(basis.degree() == 5 && basis.pole_count() == 7);
  CHECK(throws_with([] { detail::EndInterpolation({0.0, 0.0}, 0, 0); }, "must increase"));
}

void test_fields() {
  // The radial unit field of a circle is an affine image of it: exact.
  const gp_Pnt center(1, 2, 3);
  const occ::handle<Geom_BSplineCurve> circle = circle_curve(center, 7);
  const std::vector<gp_Vec> field =
      detail::fit_field(*circle, [&](double u) { return gp_Vec(center, circle->Value(u)) / 7; });
  NCollection_Array1<gp_Pnt> poles(1, circle->NbPoles());
  for (int k = 1; k <= circle->NbPoles(); ++k) {
    poles(k) = gp_Pnt(field[static_cast<std::size_t>(k) - 1].XYZ());
  }
  const occ::handle<Geom_BSplineCurve> radial = new Geom_BSplineCurve(
      poles, circle->WeightsArray(), circle->Knots(), circle->Multiplicities(), circle->Degree());
  for (const double u : {0.1, 1.3, 2.9, 4.4, 6.0}) {
    CHECK(near_vec(gp_Vec(radial->Value(u).XYZ()), gp_Vec(center, circle->Value(u)) / 7, 1e-12));
  }

  // Polynomial forms of rational curves stay within the tolerance.
  const occ::handle<Geom_BSplineCurve> plain = detail::polynomial(circle, 1.0e-7);
  CHECK(!plain->IsRational());
  double worst = 0.0;
  for (int i = 0; i <= 100; ++i) {
    const double u = 2 * kPi * i / 100;
    worst = std::max(worst, plain->Value(u).Distance(circle->Value(u)));
  }
  CHECK(worst < 1.0e-7);
  const occ::handle<Geom_BSplineCurve> line = line_curve(gp_Pnt(0, 0, 0), gp_Pnt(1, 0, 0));
  CHECK(detail::polynomial(line) == line);
}

// Gordon surfaces through open and closed networks.
void test_gordon() {
  // Two circles and two straight rails: a cylinder.
  for (const bool rational : {true, false}) {
    Curves profiles;
    for (const double z : {0.0, 40.0}) {
      const occ::handle<Geom_BSplineCurve> c = circle_curve(gp_Pnt(0, 0, z), 10);
      profiles.push_back(rational ? c : detail::polynomial(c));
    }
    const Curves guides = curves({line_curve(gp_Pnt(10, 0, 0), gp_Pnt(10, 0, 40)),
                                  line_curve(gp_Pnt(-10, 0, 0), gp_Pnt(-10, 0, 40))});
    const occ::handle<Geom_BSplineSurface> surface = detail::gordon_surface(profiles, guides, 1.0e-6);
    double worst = 0.0;
    for (int i = 0; i <= 20; ++i) {
      for (int j = 0; j <= 4; ++j) {
        double u0, u1, v0, v1;
        surface->Bounds(u0, u1, v0, v1);
        const gp_Pnt p = surface->Value(u0 + (u1 - u0) * i / 20, v0 + (v1 - v0) * j / 4);
        worst = std::max(worst, std::abs(std::hypot(p.X(), p.Y()) - 10));
      }
    }
    CHECK(worst < (rational ? 1e-9 : 1e-6));
    const TopoDS_Face face = BRepBuilderAPI_MakeFace(surface, 1.0e-7).Face();
    CHECK_NEAR_TOL(area_of(face), 2 * kPi * 10 * 40, 1e-6);
  }

  // Two squares and four corner rails: each side a Gordon surface between
  // two rails, sewn with the caps into the frustum of loft_two_squares.
  {
    const auto corner = [](double h, double z, int k) {
      const double x[4] = {-h, h, h, -h};
      const double y[4] = {-h, -h, h, h};
      return gp_Pnt(x[k % 4], y[k % 4], z);
    };
    std::vector<TopoDS_Face> faces;
    for (int k = 0; k < 4; ++k) {
      const Curves profiles = curves({line_curve(corner(20, 0, k), corner(20, 0, k + 1)),
                                      line_curve(corner(10, 30, k), corner(10, 30, k + 1))});
      const Curves guides = curves({line_curve(corner(20, 0, k), corner(10, 30, k)),
                                    line_curve(corner(20, 0, k + 1), corner(10, 30, k + 1))});
      faces.push_back(BRepBuilderAPI_MakeFace(detail::gordon_surface(profiles, guides, 1.0e-6), 1.0e-7).Face());
    }
    for (const auto& [h, z] : {std::pair{20.0, 0.0}, {10.0, 30.0}}) {
      BRepBuilderAPI_MakePolygon polygon(corner(h, z, 0), corner(h, z, 1), corner(h, z, 2), corner(h, z, 3), true);
      faces.push_back(BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face());
    }
    CHECK_NEAR_TOL(std::abs(volume_of(sewn_solid(faces))), 28000, 1e-9);
  }

  // A guide that misses a profile.
  const Curves profiles = curves({line_curve(gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0)),
                                  line_curve(gp_Pnt(0, 0, 10), gp_Pnt(10, 0, 10))});
  const Curves guides = curves({line_curve(gp_Pnt(0, 0, 0), gp_Pnt(0, 0, 10)),
                                line_curve(gp_Pnt(10, 5, 0), gp_Pnt(10, 5, 10))});
  CHECK(throws_with([&] { detail::gordon_surface(profiles, guides, 1.0e-6); },
                    "no surface could be built through the curve network"));
}

Frame xy_at(double z) {
  Frame frame;
  frame.origin = gp_Pnt(0, 0, z);
  return frame;
}

LoftSection region_at(double z, const Region& region) {
  LoftSection section;
  section.frame = xy_at(z);
  section.region = region;
  return section;
}

LoftEnd condition(LoftEnd::Kind kind, double weight = 1.0, double angle = 0.0) {
  LoftEnd end;
  end.kind = kind;
  end.weight = weight;
  end.angle = angle;
  return end;
}

// The surface of the loft's face named `name` and its parameter bounds.
struct FaceSurface {
  occ::handle<Geom_Surface> surface;
  double u0, u1, v0, v1;
};

FaceSurface face_surface(const Shape& shape, const std::string& name) {
  const std::vector<int> faces = shape.find_faces(name);
  if (faces.empty()) {
    throw std::runtime_error("no face " + name);
  }
  FaceSurface result;
  const TopoDS_Face& face = shape.face(faces.front());
  result.surface = BRep_Tool::Surface(face);
  BRepTools::UVBounds(face, result.u0, result.u1, result.v0, result.v1);
  return result;
}

// The largest |n . z| along the edge of the face at its lowest v (where the
// loft leaves its first section), and the largest normal curvature across
// it there.
std::pair<double, double> leaving(const Shape& shape, const std::string& name) {
  const FaceSurface f = face_surface(shape, name);
  double tilt = 0.0;
  double bend = 0.0;
  for (int i = 1; i < 10; ++i) {
    const double u = f.u0 + (f.u1 - f.u0) * i / 10;
    gp_Pnt p;
    gp_Vec su, sv, suu, svv, suv;
    f.surface->D2(u, f.v0, p, su, sv, suu, svv, suv);
    const gp_Vec n = su.Crossed(sv).Normalized();
    tilt = std::max(tilt, std::abs(n.Z()));
    bend = std::max(bend, std::abs(svv.Dot(n)) / sv.SquareMagnitude());
  }
  return {tilt, bend};
}

// Lofts with end conditions: volumes from the interpolation's polynomials,
// with Mitcad's rules (loft_skin.cpp, far_end): over the span t from
// the conditioned section to the free one, a cubic leaving with the
// takeoff and reaching the free section with 2 c - (c.d) d (c the chord
// between corresponding points, d the unit takeoff).
void test_loft_conditions() {
  // Direction (angle 0, weight 1) at a 20 mm circle up to a 10 mm one 30 mm
  // above: the takeoff 30 t, so z = 30 t, r = 20 - 10 t^2, V = 8600 pi.
  LoftSpec cone;
  cone.feature = "F5";
  cone.sections = {region_at(0, circle(1, 0, 0, 20)), region_at(30, circle(2, 0, 0, 10))};
  cone.start = condition(LoftEnd::Kind::Direction);
  const ShapePtr bulged = loft(cone);
  CHECK_NEAR_TOL(volume(*bulged), 8600 * kPi, 1e-6);
  CHECK(faces_named(*bulged, "F5:side(c1)") == 1 && faces_named(*bulged, "F5:start(r{c1})") == 1 &&
        faces_named(*bulged, "F5:end(r{c2})") == 1 && edge_names_unique(*bulged));
  // Above weight 1 the free end's z'(1) is 30 - (w - 1)(30 - |c| / w), |c|
  // = sqrt(1000) the chord between corresponding points; the same radius.
  // Weight 2: z'(0) = 60, z'(1) = |c| / 2, V = pi (10200 - 3200 sqrt(10) / 21);
  // 1.5: z'(1) = 15 + |c| / 3; 3: z'(1) = -30 + 2 |c| / 3, below 0.
  cone.start.weight = 2;
  CHECK_NEAR_TOL(volume(*loft(cone)), kPi * (10200 - 3200 * std::sqrt(10.0) / 21), 1e-6);
  cone.start.weight = 1.5;
  CHECK_NEAR_TOL(volume(*loft(cone)), kPi * (9400 - 6400 * std::sqrt(10.0) / 63), 1e-6);
  cone.start.weight = 3;
  CHECK_NEAR_TOL(volume(*loft(cone)), kPi * (11800 - 12800 * std::sqrt(10.0) / 63), 1e-6);
  // Weight 0.5: z'(0) = 15, z'(1) = 30: z = 15 t + 30 t^2 - 15 t^3,
  // V = 57800 pi / 7.
  cone.start.weight = 0.5;
  CHECK_NEAR_TOL(volume(*loft(cone)), 57800 * kPi / 7, 1e-6);
  // At the end instead: the mirror image.
  cone.start = condition(LoftEnd::Kind::Free);
  cone.end = condition(LoftEnd::Kind::Direction);
  cone.sections = {region_at(0, circle(2, 0, 0, 10)), region_at(30, circle(1, 0, 0, 20))};
  CHECK_NEAR_TOL(volume(*loft(cone)), 8600 * kPi, 1e-6);
  // A positive angle tilts the takeoff out of the section: fuller.
  cone.end.angle = 10 * kPi / 180;
  const double tilted = volume(*loft(cone));
  CHECK(tilted > 8600 * kPi * 1.01);

  // Three sections, conditions at both ends (degree 5 with a smooth one):
  // symmetric about the middle section.
  LoftSpec waist;
  waist.feature = "F5";
  waist.sections = {region_at(0, circle(1, 0, 0, 20)), region_at(20, circle(2, 0, 0, 10)),
                    region_at(40, circle(3, 0, 0, 20))};
  waist.start = condition(LoftEnd::Kind::Direction);
  waist.end = condition(LoftEnd::Kind::Direction);
  const MassProperties pinched = mass_properties(*loft(waist));
  CHECK(near(pinched.center.Z(), 20, 1e-9) && pinched.volume > kPi * 100 * 40 && pinched.volume < kPi * 400 * 40);
  // A square to a circle: the circle is cut where the square's corners
  // fall, and the square where the circle starts (its right side in two
  // pieces); the curves of different kinds are lofted in polynomial form.
  LoftSpec rounded_off;
  rounded_off.feature = "F5";
  rounded_off.sections = {region_at(0, rectangle(1, -20, -20, 40, 40)), region_at(30, circle(5, 0, 0, 10))};
  rounded_off.start = condition(LoftEnd::Kind::Direction, 1.0, 10 * kPi / 180);
  const ShapePtr transition = loft(rounded_off);
  CHECK(transition->face_count() == 7 && faces_named(*transition, side("F5", 1, 2)) == 1 &&
        faces_named(*transition, side("F5", 1, 1) + "#1") == 1 &&
        faces_named(*transition, "F5:end(r{c5})") == 1 && edge_names_unique(*transition));

  // Squares with corners: the takeoff at an angle leaves each corner with
  // one vector, so the faces still close into a solid.
  LoftSpec squares;
  squares.feature = "F5";
  squares.sections = {region_at(0, rectangle(1, -20, -20, 40, 40)), region_at(30, rectangle(5, -10, -10, 20, 20))};
  squares.start = condition(LoftEnd::Kind::Direction);
  CHECK_NEAR_TOL(volume(*loft(squares)), 34400, 1e-6);
  // Weight 2: z'(1) = |c| / 2 with the corners' chord sqrt(1100) along the
  // whole of each face, as in the reference model loft_direction_squares_w2.
  squares.start.weight = 2;
  CHECK_NEAR_TOL(volume(*loft(squares)), 40800 - 1280 * std::sqrt(1100.0) / 21, 1e-6);
  squares.start.weight = 1;
  squares.start.angle = -15 * kPi / 180;
  squares.end = condition(LoftEnd::Kind::Direction, 1.5, 20 * kPi / 180);
  const ShapePtr twisted = loft(squares);
  CHECK(twisted->face_count() == 6 && edge_names_unique(*twisted));
  CHECK(faces_named(*twisted, side("F5", 1, 0)) == 1 && faces_named(*twisted, start_cap("F5", 1)) == 1);

  // Tangent to the walls of a 40 x 40 x 20 block from its top to a 20 mm
  // square at z = 50 (the reference model loft_tangent_face): the takeoff
  // as long as the corners are apart, M = sqrt(1100); the half width
  // 20 - 10 t^2, z' = M + (120 - 4 M) t + (3 M - 90) t^2:
  // V = (640 M + 221600) / 7.
  const ShapePtr block = extrude("F2", {rectangle(1, -20, -20, 40, 40)}, 0, 20);
  LoftSpec onto;
  onto.feature = "F7";
  LoftSection top;
  top.kind = LoftSection::Kind::Face;
  top.body = block;
  top.face = end_cap("F2", 1);
  onto.sections = {top, region_at(50, rectangle(5, -10, -10, 20, 20))};
  onto.start = condition(LoftEnd::Kind::Tangent);
  const ShapePtr tangent = loft(onto);
  const double corners = std::sqrt(1100.0);
  CHECK_NEAR_TOL(volume(*tangent), (640 * corners + 221600) / 7, 1e-6);
  const std::string wall = "F7:side(" + edge(side("F2", 1, 0), end_cap("F2", 1)) + ")";
  CHECK(faces_named(*tangent, wall) == 1 && faces_named(*tangent, "F7:start(" + end_cap("F2", 1) + ")") == 1);
  CHECK(leaving(*tangent, wall).first < 1e-9);
  // Smooth (loft_smooth_face): the walls are flat, so the curvature across
  // is 0 too; quintic, the half width 20 - 30 t^3 + 30 t^4 - 10 t^5, the
  // height as the tangent loft's: V = (270700 M + 3244760 * 30) / 3003.
  onto.start = condition(LoftEnd::Kind::Smooth);
  const ShapePtr smooth = loft(onto);
  CHECK_NEAR_TOL(volume(*smooth), (270700 * corners + 3244760.0 * 30) / 3003, 1e-6);
  const auto [tilt, bend] = leaving(*smooth, wall);
  CHECK(tilt < 1e-9 && bend < 1e-9);
  CHECK(edge_names_unique(*smooth));

  // Tangent to a curved wall: from the top of a 10 mm cylinder 20 high to
  // a 5 mm circle 20 above it, M = sqrt(425), r = 10 - 5 t^2:
  // V = pi (40 M / 7 + 27700 / 21); Smooth, the wall straight upwards,
  // r = 10 - 15 t^3 + 15 t^4 - 5 t^5: V = pi (67675 M / 12012 + 405595 * 20 / 6006).
  const ShapePtr post = extrude("F2", {circle(1, 0, 0, 10)}, 0, 20);
  LoftSpec up;
  up.feature = "F7";
  LoftSection disc;
  disc.kind = LoftSection::Kind::Face;
  disc.body = post;
  disc.face = "F2:end(r{c1})";
  up.sections = {disc, region_at(40, circle(3, 0, 0, 5))};
  up.start = condition(LoftEnd::Kind::Tangent);
  const double seam = std::sqrt(425.0);
  CHECK_NEAR_TOL(volume(*loft(up)), kPi * (40 * seam / 7 + 27700.0 / 21), 1e-6);
  up.start = condition(LoftEnd::Kind::Smooth);
  CHECK_NEAR_TOL(volume(*loft(up)), kPi * (67675 * seam / 12012 + 405595.0 * 20 / 6006), 1e-6);

  // Tangent to slanted walls: from the top of the frustum of
  // loft_two_squares; the loft leaves along them (its corner lines along
  // the frustum's edges).
  LoftSpec frustum;
  frustum.feature = "F3";
  frustum.sections = {region_at(0, rectangle(1, -20, -20, 40, 40)), region_at(30, rectangle(5, -10, -10, 20, 20))};
  const ShapePtr pyramid = loft(frustum);
  LoftSpec beyond;
  beyond.feature = "F7";
  LoftSection roof;
  roof.kind = LoftSection::Kind::Face;
  roof.body = pyramid;
  roof.face = end_cap("F3", 5);
  beyond.sections = {roof, region_at(50, rectangle(9, -5, -5, 10, 10))};
  beyond.start = condition(LoftEnd::Kind::Tangent);
  const ShapePtr steeple = loft(beyond);
  CHECK(steeple->face_count() == 6 && edge_names_unique(*steeple));
  {
    // Along the middle of an edge the side face continues the wall's plane.
    const std::string name = "F7:side(" + edge(side("F3", 1, 0), end_cap("F3", 5)) + ")";
    const FaceSurface f = face_surface(*steeple, name);
    gp_Pnt p;
    gp_Vec su, sv;
    f.surface->D1((f.u0 + f.u1) / 2, f.v0, p, su, sv);
    const gp_Vec slant = gp_Vec(gp_Pnt(-20, -20, 0), gp_Pnt(20, -20, 0)).Crossed(gp_Vec(gp_Pnt(-20, -20, 0), gp_Pnt(-10, -10, 30)));
    CHECK(std::abs(su.Crossed(sv).Normalized().Dot(slant.Normalized())) > 1 - 1e-9);
  }

  // A rounded tip (loft_point_tangent): a 10 mm circle up to a point 20 mm
  // above, the tip leaving each point by its distance from the axis per
  // the span: r = 10 (1 - t), z = 20 - 20 (1 - t)^2, the paraboloid,
  // V = 1000 pi, the tip's tangent plane level.
  LoftSpec dome;
  dome.feature = "F5";
  LoftSection tip;
  tip.kind = LoftSection::Kind::Point;
  tip.point = gp_Pnt(0, 0, 20);
  dome.sections = {region_at(0, circle(1, 0, 0, 10)), tip};
  dome.end = condition(LoftEnd::Kind::PointTangent);
  const ShapePtr rounded = loft(dome);
  CHECK_NEAR_TOL(volume(*rounded), 1000 * kPi, 1e-6);
  {
    const FaceSurface f = face_surface(*rounded, "F5:side(c1)");
    gp_Pnt p;
    gp_Vec su, sv;
    f.surface->D1((f.u0 + f.u1) / 3, f.v1, p, su, sv);
    CHECK(near_point(p, gp_Pnt(0, 0, 20)) && std::abs(sv.Z()) < 1e-9);
  }

  // Conditions that do not fit their sections, and options they exclude.
  LoftSpec wrong = cone;
  wrong.end = condition(LoftEnd::Kind::Tangent);
  CHECK(throws_with([&] { loft(wrong); }, "need a face of a body"));
  wrong.end = condition(LoftEnd::Kind::PointTangent);
  CHECK(throws_with([&] { loft(wrong); }, "does not fit a curve section"));
  wrong.end = condition(LoftEnd::Kind::Direction, 0);
  CHECK(throws_with([&] { loft(wrong); }, "weight must be positive"));
  wrong.end = condition(LoftEnd::Kind::Direction);
  wrong.ruled = true;
  CHECK(throws_with([&] { loft(wrong); }, "a ruled loft has no end conditions or rails"));
  wrong.ruled = false;
  wrong.centerline = {{"c9", ModelCurve{}}};
  wrong.centerline.front().curve.kind = ModelCurveKind::Line;
  wrong.centerline.front().curve.end = gp_Pnt(0, 0, 30);
  CHECK(throws_with([&] { loft(wrong); }, "unsupported: end conditions of a loft along a centre line"));
}

Path line_rail(const gp_Pnt& a, const gp_Pnt& b) {
  PathCurve piece;
  piece.name = "c9";
  piece.curve.kind = ModelCurveKind::Line;
  piece.curve.start = a;
  piece.curve.end = b;
  return {piece};
}

// A quadratic Bezier rail.
Path bent_rail(const gp_Pnt& a, const gp_Pnt& middle, const gp_Pnt& b) {
  PathCurve piece;
  piece.name = "c9";
  piece.curve.kind = ModelCurveKind::BSpline;
  piece.curve.degree = 2;
  piece.curve.poles = {a, middle, b};
  piece.curve.knots = {0, 0, 0, 1, 1, 1};
  return {piece};
}

// The largest angle between the u derivatives where a closed face meets
// itself (at its seam).
double seam_angle(const Shape& shape, const std::string& name) {
  const FaceSurface f = face_surface(shape, name);
  double worst = 0.0;
  for (int j = 0; j <= 8; ++j) {
    const double v = f.v0 + (f.v1 - f.v0) * j / 8;
    gp_Pnt a, b;
    gp_Vec au, av, bu, bv;
    f.surface->D1(f.u0, v, a, au, av);
    f.surface->D1(f.u1, v, b, bu, bv);
    worst = std::max(worst, au.Angle(bu));
  }
  return worst;
}

// Lofts through rails.
void test_loft_rails() {
  // loft_two_squares with straight rails at the corners: the same frustum,
  // faces and names.
  LoftSpec frustum;
  frustum.feature = "F5";
  frustum.sections = {region_at(0, rectangle(1, -20, -20, 40, 40)), region_at(30, rectangle(5, -10, -10, 20, 20))};
  for (const auto& [x, y] : {std::pair{-1.0, -1.0}, {1.0, -1.0}, {1.0, 1.0}, {-1.0, 1.0}}) {
    frustum.rails.push_back(line_rail(gp_Pnt(20 * x, 20 * y, 0), gp_Pnt(10 * x, 10 * y, 30)));
  }
  const ShapePtr railed = loft(frustum);
  CHECK_NEAR_TOL(volume(*railed), 28000, 1e-9);
  CHECK(railed->face_count() == 6 && faces_named(*railed, side("F5", 1, 0)) == 1 &&
        faces_named(*railed, start_cap("F5", 1)) == 1 && faces_named(*railed, end_cap("F5", 5)) == 1);
  CHECK(edge_names_unique(*railed));
  // A rail through the middles of two edges splits them.
  LoftSpec middle = frustum;
  middle.rails = {line_rail(gp_Pnt(0, -20, 0), gp_Pnt(0, -10, 30))};
  const ShapePtr split = loft(middle);
  CHECK_NEAR_TOL(volume(*split), 28000, 1e-9);
  CHECK(faces_named(*split, side("F5", 1, 0) + "#0") == 1 && faces_named(*split, side("F5", 1, 0) + "#1") == 1);

  // Circles and two straight rails on their cylinder: the cylinder, the
  // side in two halves.
  LoftSpec tube;
  tube.feature = "F5";
  tube.sections = {region_at(0, circle(1, 0, 0, 10)), region_at(40, circle(2, 0, 0, 10))};
  tube.rails = {line_rail(gp_Pnt(10, 0, 0), gp_Pnt(10, 0, 40)), line_rail(gp_Pnt(-10, 0, 0), gp_Pnt(-10, 0, 40))};
  const ShapePtr cylinder = loft(tube);
  CHECK_NEAR_TOL(volume(*cylinder), kPi * 100 * 40, 1e-6);
  CHECK(faces_named(*cylinder, "F5:side(c1)#0") == 1 && faces_named(*cylinder, "F5:side(c1)#1") == 1);

  // One bent rail through the circles' start points (the reference model
  // loft_rail_circle_one): the circles move along it, so the loft is the
  // cylinder shifted (its volume stays), smooth at its seam along the rail,
  // symmetric about XZ.
  tube.rails = {bent_rail(gp_Pnt(10, 0, 0), gp_Pnt(16, 0, 20), gp_Pnt(10, 0, 40))};
  const ShapePtr bulge = loft(tube);
  CHECK(bulge->face_count() == 3 && faces_named(*bulge, "F5:side(c1)") == 1);
  CHECK(seam_angle(*bulge, "F5:side(c1)") < 1e-6);
  // (Bent, the faces are polynomial B-splines of high degree, which the
  // fixed Gauss points of volume() integrate only to about 1e-4.)
  const MassProperties props = mass_properties(*bulge);
  CHECK_NEAR_TOL(volume_of(bulge->occt()), kPi * 100 * 40, 1e-3);
  CHECK(std::abs(props.center.Y()) < 1e-6 && props.center.X() > 0);
  CHECK(near(bounding_box(*bulge).max.X(), 13, 1e-6) && near(bounding_box(*bulge).min.X(), -10, 1e-6));
  {
    // Halfway up, the far side has moved by as much as the rail.
    const FaceSurface f = face_surface(*bulge, "F5:side(c1)");
    gp_Pnt near_side;
    gp_Pnt far_side;
    const double halfway = (f.v0 + f.v1) / 2;
    f.surface->D0(f.u0, halfway, near_side);
    f.surface->D0((f.u0 + f.u1) / 2, halfway, far_side);
    CHECK(near(near_side.X() - far_side.X(), 20, 1e-6) && near(far_side.Z(), near_side.Z(), 1e-6));
  }

  // Bent rails on all corners of the squares.
  for (std::size_t r = 0; r < frustum.rails.size(); ++r) {
    const gp_Pnt a = frustum.rails[r].front().curve.start;
    const gp_Pnt b = frustum.rails[r].front().curve.end;
    const gp_Pnt mid((a.X() + b.X()) * 0.6, (a.Y() + b.Y()) * 0.6, 15);
    frustum.rails[r] = bent_rail(a, mid, b);
  }
  const ShapePtr bowed = loft(frustum);
  CHECK(bowed->face_count() == 6 && volume(*bowed) > 28000 && edge_names_unique(*bowed));

  // Rails that miss a section, and with a point.
  LoftSpec missing = tube;
  missing.rails = {line_rail(gp_Pnt(10, 0, 0), gp_Pnt(12, 0, 40))};
  CHECK(throws_with([&] { loft(missing); }, "rail 1 does not meet section 2"));
  LoftSpec pointed = tube;
  pointed.sections[1].kind = LoftSection::Kind::Point;
  pointed.sections[1].point = gp_Pnt(0, 0, 40);
  pointed.rails = {line_rail(gp_Pnt(10, 0, 0), gp_Pnt(0, 0, 40))};
  CHECK(throws_with([&] { loft(pointed); }, "unsupported: loft rails with a point section"));
}

} // namespace

void skin_tests() {
  guarded("interpolation with end derivatives", test_interpolation);
  guarded("fields along curves", test_fields);
  guarded("gordon surfaces", test_gordon);
  guarded("lofts with end conditions", test_loft_conditions);
  guarded("lofts with rails", test_loft_rails);
}

} // namespace test
