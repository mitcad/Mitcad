// SPDX-License-Identifier: MIT
// Geometry analysis tests against analytic values.

#include "mitcad/analysis/common.hpp"
#include "mitcad/analysis/compare.hpp"
#include "mitcad/analysis/interference.hpp"
#include "mitcad/analysis/measure.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/analysis/section.hpp"

#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Wire.hxx>
#include <BRepLib.hxx>
#include <GProp_GProps.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_OffsetCurve.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <gp_GTrsf.hxx>
#include <math.hxx>
#include <math_Vector.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec2d.hxx>
#include <gp_Trsf.hxx>

namespace {

constexpr double kPi = 3.14159265358979323846;
int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "test_analysis.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// Relative closeness for values of any size.
bool near(double a, double b, double tolerance = 1e-9) {
  return std::abs(a - b) <= tolerance * std::max(1.0, std::abs(b));
}

bool near_vec(const mitcad::analysis::Vec3& a, double x, double y, double z,
              double tolerance = 1e-9) {
  return near(a.x, x, tolerance) && near(a.y, y, tolerance) && near(a.z, z, tolerance);
}

// A unit vector parallel (or antiparallel) to (x, y, z).
bool parallel(const mitcad::analysis::Vec3& a, double x, double y, double z) {
  return std::abs(std::abs(a.x * x + a.y * y + a.z * z) - 1.0) < 1e-9;
}

template <class F>
bool throws(F&& f) {
  try {
    f();
  } catch (const mitcad::analysis::Error&) {
    return true;
  }
  return false;
}

using mitcad::analysis::Plane;

TopoDS_Shape box(double x, double y, double z, double dx, double dy, double dz) {
  return BRepPrimAPI_MakeBox(gp_Pnt(x, y, z), dx, dy, dz).Shape();
}

TopoDS_Shape moved(const TopoDS_Shape& shape, const gp_Trsf& transform) {
  return BRepBuilderAPI_Transform(shape, transform, true).Shape();
}

TopoDS_Face face_with_normal(const TopoDS_Shape& shape, double x, double y, double z) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane) {
      continue;
    }
    // Box faces: the outward normal is the plane axis, flipped for reversed faces.
    gp_Dir normal = BRepAdaptor_Surface(face).Plane().Axis().Direction();
    if (face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    if (normal.IsParallel(gp_Dir(x, y, z), 1e-9) && normal.Dot(gp_Dir(x, y, z)) > 0.0) {
      return face;
    }
  }
  return TopoDS_Face();
}

TopoDS_Edge circular_edge(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    if (BRepAdaptor_Curve(TopoDS::Edge(it.Current())).GetType() == GeomAbs_Circle) {
      return TopoDS::Edge(it.Current());
    }
  }
  return TopoDS_Edge();
}

TopoDS_Face face_of_type(const TopoDS_Shape& shape, GeomAbs_SurfaceType type) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(it.Current())).GetType() == type) {
      return TopoDS::Face(it.Current());
    }
  }
  return TopoDS_Face();
}

void test_box_properties() {
  const double a = 10.0;
  const double b = 20.0;
  const double c = 30.0;
  const double density = 7.85; // steel, g/cm^3
  const auto p = mitcad::analysis::physical_properties(box(0, 0, 0, a, b, c), density);
  const double m = a * b * c * density * 1e-6;
  CHECK(near(p.volume, 6000.0));
  CHECK(near(p.area, 2200.0));
  CHECK(near(p.mass, m));
  CHECK(near_vec(p.center_of_mass, 5.0, 10.0, 15.0));
  CHECK(near(p.inertia[0][0], m * (b * b + c * c) / 12.0));
  CHECK(near(p.inertia[1][1], m * (a * a + c * c) / 12.0));
  CHECK(near(p.inertia[2][2], m * (a * a + b * b) / 12.0));
  CHECK(std::abs(p.inertia[0][1]) < 1e-12 && std::abs(p.inertia[1][2]) < 1e-12);
  // Ascending principal moments: about z, y, x.
  CHECK(near(p.principal_moments[0], m * (a * a + b * b) / 12.0));
  CHECK(near(p.principal_moments[2], m * (b * b + c * c) / 12.0));
  CHECK(parallel(p.principal_axes[0], 0, 0, 1));
  CHECK(parallel(p.principal_axes[1], 0, 1, 0));
  CHECK(parallel(p.principal_axes[2], 1, 0, 0));
  CHECK(!p.bounds.empty && near_vec(p.bounds.max, 10.0, 20.0, 30.0));

  // Turned 30 degrees about z: same moments, rotated axes.
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp::Origin(), gp::DZ()), kPi / 6.0);
  const auto r = mitcad::analysis::physical_properties(moved(box(0, 0, 0, a, b, c), turn), density);
  for (int i = 0; i < 3; ++i) {
    CHECK(near(r.principal_moments[static_cast<std::size_t>(i)],
               p.principal_moments[static_cast<std::size_t>(i)], 1e-9));
  }
  CHECK(parallel(r.principal_axes[2], std::cos(kPi / 6.0), std::sin(kPi / 6.0), 0.0));
  CHECK(near(r.inertia[0][1], r.inertia[1][0]));
  CHECK(std::abs(r.inertia[0][1]) > 1e-6);
}

void test_round_properties() {
  const double radius = 5.0;
  const double height = 20.0;
  const auto cyl = mitcad::analysis::physical_properties(
      BRepPrimAPI_MakeCylinder(radius, height).Shape(), 2.7);
  const double v = kPi * radius * radius * height;
  const double m = v * 2.7e-6;
  CHECK(near(cyl.volume, v, 1e-9));
  CHECK(near(cyl.area, 2.0 * kPi * radius * (radius + height), 1e-9));
  CHECK(near_vec(cyl.center_of_mass, 0.0, 0.0, 10.0, 1e-9));
  CHECK(near(cyl.inertia[2][2], m * radius * radius / 2.0, 1e-9));
  CHECK(near(cyl.inertia[0][0], m * (3.0 * radius * radius + height * height) / 12.0, 1e-9));

  const double r = 10.0;
  const auto sphere = mitcad::analysis::physical_properties(
      BRepPrimAPI_MakeSphere(gp_Pnt(1.0, 2.0, 3.0), r).Shape());
  const double ms = 4.0 / 3.0 * kPi * r * r * r * 1e-6;
  CHECK(near(sphere.volume, 4.0 / 3.0 * kPi * r * r * r, 1e-9));
  CHECK(near(sphere.area, 4.0 * kPi * r * r, 1e-9));
  CHECK(near(sphere.mass, ms, 1e-9));
  CHECK(near_vec(sphere.center_of_mass, 1.0, 2.0, 3.0, 1e-9));
  for (std::size_t i = 0; i < 3; ++i) {
    CHECK(near(sphere.principal_moments[i], 0.4 * ms * r * r, 1e-9));
  }
}

// Faces bounded by B-splines of many spans (mitcad#139): a hole through a
// cylinder of a stored body was bounded by a cubic of 132 spans fitted to
// the intersection, and the face measured 141 mm² too small with OCCT's
// fixed Gauss rule (at most 61 points along a boundary curve, not split at
// its knots, integrated from the far side of the face to the curve, so its
// error on the curve is multiplied by that distance). Faces are integrated
// span by span (mitcad#140). Here a plate of 500 mm with a hole near one
// side bounded by a closed cubic of 132 spans through points alternately
// 0.01 mm out of and into a circle, whose area is exact span by span
// (x y' is a polynomial of degree 5 on each), and a thread's band, bounded
// by helices that are lines in the cylinder's parameter space stored as
// cubics of 200 spans.
void test_spline_bounded_properties() {
  const double radius = 25.0;
  const double cx = 450.0;
  const double cy = 250.0;
  const int count = 132;
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, count);
  for (int i = 0; i < count; ++i) {
    const double angle = 2.0 * kPi * i / count;
    const double r = radius + (i % 2 == 0 ? 0.01 : -0.01);
    points->SetValue(i + 1, gp_Pnt(cx + r * std::cos(angle), cy + r * std::sin(angle), 0.0));
  }
  GeomAPI_Interpolate interpolate(points, true, 1e-9);
  interpolate.Perform();
  CHECK(interpolate.IsDone());
  const occ::handle<Geom_BSplineCurve> curve = interpolate.Curve();
  CHECK(curve->Degree() == 3 && curve->NbKnots() > 61);
  math_Vector gauss(1, 4);
  math_Vector weights(1, 4);
  math::GaussPoints(4, gauss);
  math::GaussWeights(4, weights);
  double hole = 0.0;
  for (int k = 1; k < curve->NbKnots(); ++k) {
    const double a = curve->Knot(k);
    const double b = curve->Knot(k + 1);
    for (int g = 1; g <= 4; ++g) {
      gp_Pnt p;
      gp_Vec d;
      curve->D1(0.5 * (a + b) + 0.5 * (b - a) * gauss(g), p, d);
      hole += weights(g) * 0.5 * (b - a) * (p.X() - cx) * d.Y();
    }
  }
  CHECK(near(hole, kPi * radius * radius, 1e-5));
  BRepBuilderAPI_MakeFace plate(gp_Pln(gp::XOY()),
                                BRepBuilderAPI_MakePolygon(gp_Pnt(0, 0, 0), gp_Pnt(500, 0, 0),
                                                           gp_Pnt(500, 500, 0), gp_Pnt(0, 500, 0), true)
                                    .Wire());
  plate.Add(TopoDS::Wire(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(curve).Edge()).Wire().Reversed()));
  CHECK(plate.IsDone());
  const GProp_GProps p = mitcad::analysis::surface_properties(plate.Face());
  CHECK(near(p.Mass(), 500.0 * 500.0 - hole, 1e-10));

  // The band: 1 rad wide, two turns up 20 mm on a cylinder of 5 mm, its
  // area 5 mm times its parallelogram's in the parameter space.
  const double r = 5.0;
  const double width = 1.0;
  const double turns = 4.0 * kPi;
  const double height = 20.0;
  const occ::handle<Geom_CylindricalSurface> cylinder = new Geom_CylindricalSurface(gp::XOY(), r);
  const auto helix = [&](const gp_Pnt2d& from) {
    const gp_Vec2d along(turns, height);
    occ::handle<Geom2d_BSplineCurve> spline = Geom2dConvert::CurveToBSplineCurve(
        new Geom2d_TrimmedCurve(new Geom2d_Line(from, gp_Dir2d(along)), 0.0, along.Magnitude()));
    spline->IncreaseDegree(3);
    for (int k = 1; k < 200; ++k) {
      spline->InsertKnot(along.Magnitude() * k / 200.0);
    }
    return spline;
  };
  const auto line = [&](const gp_Pnt2d& from, const gp_Pnt2d& to) {
    return occ::handle<Geom2d_Curve>(new Geom2d_TrimmedCurve(
        new Geom2d_Line(from, gp_Dir2d(gp_Vec2d(from, to))), 0.0, from.Distance(to)));
  };
  const gp_Pnt2d a(0.0, 0.0);
  const gp_Pnt2d b(width, 0.0);
  const gp_Pnt2d c(width + turns, height);
  const gp_Pnt2d d(turns, height);
  const occ::handle<Geom2d_BSplineCurve> left = helix(a);
  CHECK(left->NbKnots() > 61);
  BRepBuilderAPI_MakeWire band;
  band.Add(BRepBuilderAPI_MakeEdge(line(a, b), cylinder).Edge());
  band.Add(BRepBuilderAPI_MakeEdge(helix(b), cylinder).Edge());
  band.Add(BRepBuilderAPI_MakeEdge(line(c, d), cylinder).Edge());
  band.Add(BRepBuilderAPI_MakeEdge(left->Reversed(), cylinder).Edge());
  CHECK(band.IsDone());
  TopoDS_Wire wire = band.Wire();
  BRepLib::BuildCurves3d(wire);
  const BRepBuilderAPI_MakeFace face(cylinder, wire);
  CHECK(face.IsDone());
  const GProp_GProps q = mitcad::analysis::surface_properties(face.Face());
  CHECK(near(q.Mass(), r * width * height, 1e-9));
}

// The integral of f over [a, b] with n Gauss points.
template <class F>
double gauss_sum(int n, double a, double b, const F& f) {
  math_Vector points(1, n);
  math_Vector weights(1, n);
  math::GaussPoints(n, points);
  math::GaussWeights(n, weights);
  double sum = 0.0;
  for (int i = 1; i <= n; ++i) {
    sum += weights(i) * 0.5 * (b - a) * f(0.5 * (a + b) + 0.5 * (b - a) * points(i));
  }
  return sum;
}

// The integral of f(t) over the curve's spans, n points on each: exact for
// the polynomials of a polynomial curve, to rounding for its arc length
// with 30.
template <class Curve, class F>
double span_sum(const occ::handle<Curve>& curve, int n, const F& f) {
  double sum = 0.0;
  for (int k = 1; k < curve->NbKnots(); ++k) {
    sum += gauss_sum(n, curve->Knot(k), curve->Knot(k + 1), f);
  }
  return sum;
}

occ::handle<Geom2d_Curve> segment(const gp_Pnt2d& from, const gp_Pnt2d& to) {
  return new Geom2d_TrimmedCurve(new Geom2d_Line(from, gp_Dir2d(gp_Vec2d(from, to))), 0.0, from.Distance(to));
}

// A line in the parameter space as a cubic of `spans` spans, as a thread's
// helix is stored.
occ::handle<Geom2d_BSplineCurve> spline_segment(const gp_Pnt2d& from, const gp_Pnt2d& to, int spans) {
  const double length = from.Distance(to);
  occ::handle<Geom2d_BSplineCurve> spline = Geom2dConvert::CurveToBSplineCurve(segment(from, to));
  spline->IncreaseDegree(3);
  NCollection_Array1<double> knots(1, spans - 1);
  NCollection_Array1<int> multiplicities(1, spans - 1);
  for (int k = 1; k < spans; ++k) {
    knots(k) = length * k / spans;
    multiplicities(k) = 1;
  }
  spline->InsertKnots(knots, multiplicities);
  return spline;
}

// A face of `surface` bounded by the curves in its parameter space, in turn.
TopoDS_Face face_from_pcurves(const occ::handle<Geom_Surface>& surface,
                              const std::vector<occ::handle<Geom2d_Curve>>& curves) {
  BRep_Builder builder;
  TopoDS_Wire wire;
  builder.MakeWire(wire);
  for (const auto& curve : curves) {
    builder.Add(wire, BRepBuilderAPI_MakeEdge(curve, surface).Edge());
  }
  TopoDS_Face face;
  builder.MakeFace(face, surface, 1e-7);
  builder.Add(face, wire);
  BRepLib::BuildCurves3d(face);
  return face;
}

// A planar face turned so that its normal points along `outward`.
TopoDS_Face facing(const TopoDS_Face& face, const gp_Dir& outward) {
  gp_Dir normal = BRepAdaptor_Surface(face).Plane().Axis().Direction();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal.Dot(outward) < 0.0 ? TopoDS::Face(face.Reversed()) : face;
}

TopoDS_Compound compound_of(const std::vector<TopoDS_Shape>& shapes) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const auto& shape : shapes) {
    builder.Add(compound, shape);
  }
  return compound;
}

// A closed cubic through `count` points around an ellipse, every other
// point `wobble` out of it.
occ::handle<Geom_BSplineCurve> closed_spline(double cx, double cy, double a, double b, int count,
                                            double wobble) {
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, count);
  for (int i = 0; i < count; ++i) {
    const double angle = 2.0 * kPi * i / count;
    const double out = i % 2 == 0 ? wobble : -wobble;
    points->SetValue(i + 1, gp_Pnt(cx + (a + out) * std::cos(angle), cy + (b + out) * std::sin(angle), 0.0));
  }
  GeomAPI_Interpolate interpolate(points, true, 1e-9);
  interpolate.Perform();
  return interpolate.Curve();
}

gp_Pnt as_pnt(const mitcad::analysis::Vec3& v) { return gp_Pnt(v.x, v.y, v.z); }

bool near_point(const gp_Pnt& a, const gp_Pnt& b, double size, double tolerance = 1e-9) {
  return a.Distance(b) <= tolerance * size;
}

// Measures against exact values (mitcad#140): each face is integrated span
// by span, so these come out within 1e-9 where OCCT's fixed Gauss rule
// (61 points across a whole face) and its adaptive rule (which refines
// along the boundary curves only) did not.
void test_span_by_span_properties() {
  // Bands on a cylinder of 5 mm between helices over 2.25 turns up 20 mm,
  // cubics of 2574 spans: 1 rad wide, and 2 pi wide, the whole side of a
  // cylinder, closed by its two discs.
  const double r = 5.0;
  const double height = 20.0;
  const double turns = 4.5 * kPi;
  const occ::handle<Geom_CylindricalSurface> cylinder = new Geom_CylindricalSurface(gp::XOY(), r);
  const auto band = [&](double width) {
    const gp_Pnt2d a(0.0, 0.0);
    const gp_Pnt2d b(width, 0.0);
    const gp_Pnt2d c(width + turns, height);
    const gp_Pnt2d d(turns, height);
    return face_from_pcurves(cylinder, {segment(a, b), spline_segment(b, c, 2574), segment(c, d),
                                        spline_segment(d, a, 2574)});
  };
  const double width = 1.0;
  const GProp_GProps narrow = mitcad::analysis::surface_properties(band(width));
  const double k = turns / height;
  const double cosines = (-std::cos(turns + width) + std::cos(turns) + std::cos(width) - 1.0) / k;
  const double sines = (std::sin(turns) - std::sin(turns + width) + std::sin(width)) / k;
  CHECK(near(narrow.Mass(), r * width * height));
  CHECK(near_point(narrow.CentreOfMass(),
                   gp_Pnt(r * cosines / (width * height), r * sines / (width * height), height / 2.0), height));
  const TopoDS_Shape solid = BRepPrimAPI_MakeCylinder(r, height).Shape();
  std::vector<TopoDS_Shape> faces{band(2.0 * kPi)};
  for (TopExp_Explorer it(solid, TopAbs_FACE); it.More(); it.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(it.Current())).GetType() == GeomAbs_Plane) {
      faces.push_back(it.Current());
    }
  }
  const GProp_GProps inside = mitcad::analysis::volume_properties(compound_of(faces));
  CHECK(near(inside.Mass(), kPi * r * r * height));
  CHECK(near(mitcad::analysis::surface_properties(compound_of(faces)).Mass(), 2.0 * kPi * r * (r + height)));
  CHECK(near_point(inside.CentreOfMass(), gp_Pnt(0.0, 0.0, height / 2.0), height));

  // A prism 40 mm high over a closed cubic of 200 spans around an ellipse,
  // its side a surface of extrusion: its volume, centre and area from the
  // spans of the curve (x y', x² y' and y² x' are polynomials on each).
  const occ::handle<Geom_BSplineCurve> curve = closed_spline(100.0, 50.0, 30.0, 20.0, 200, 0.01);
  CHECK(curve->NbKnots() > 200);
  const auto at = [&](double t) {
    gp_Pnt p;
    gp_Vec d;
    curve->D1(t, p, d);
    return std::make_pair(p, d);
  };
  const double area = span_sum(curve, 8, [&](double t) { const auto [p, d] = at(t); return p.X() * d.Y(); });
  const double mx = span_sum(curve, 8, [&](double t) { const auto [p, d] = at(t); return 0.5 * p.X() * p.X() * d.Y(); });
  const double my = span_sum(curve, 8, [&](double t) { const auto [p, d] = at(t); return -0.5 * p.Y() * p.Y() * d.X(); });
  const double length = span_sum(curve, 30, [&](double t) { return at(t).second.Magnitude(); });
  const double h = 40.0;
  const TopoDS_Face profile =
      BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(curve).Edge()).Wire()).Face();
  const TopoDS_Shape prism = BRepPrimAPI_MakePrism(profile, gp_Vec(0.0, 0.0, h)).Shape();
  const auto p = mitcad::analysis::physical_properties(prism);
  CHECK(near(p.volume, area * h));
  CHECK(near(p.area, 2.0 * area + length * h));
  CHECK(near_point(as_pnt(p.center_of_mass), gp_Pnt(mx / area, my / area, h / 2.0), 100.0));
  // The same as B-spline surfaces, the side of 200 spans across.
  const TopoDS_Shape nurbs = BRepBuilderAPI_NurbsConvert(prism).Shape();
  const auto n = mitcad::analysis::physical_properties(nurbs);
  CHECK(near(n.volume, area * h));
  CHECK(near(n.area, 2.0 * area + length * h));
  CHECK(near_point(as_pnt(n.center_of_mass), gp_Pnt(mx / area, my / area, h / 2.0), 100.0));

  // The side offset 1.5 mm out, a surface of 200 spans whose curve is not
  // a polynomial, closed by planes: against its curve's spans (30 points
  // integrate an offset span to rounding).
  const double offset = 1.5;
  const occ::handle<Geom_BSplineCurve> oval = closed_spline(0.0, 0.0, 30.0, 20.0, 200, 0.0);
  const occ::handle<Geom_OffsetCurve> out = new Geom_OffsetCurve(oval, offset, gp::DZ());
  const auto out_at = [&](double t) {
    gp_Pnt q;
    gp_Vec d;
    out->D1(t, q, d);
    return std::make_pair(q, d);
  };
  const double out_area = span_sum(oval, 30, [&](double t) { const auto [q, d] = out_at(t); return q.X() * d.Y(); });
  const double out_length = span_sum(oval, 30, [&](double t) { return out_at(t).second.Magnitude(); });
  const double out_mx = span_sum(oval, 30, [&](double t) { const auto [q, d] = out_at(t); return 0.5 * q.X() * q.X() * d.Y(); });
  const double out_my = span_sum(oval, 30, [&](double t) { const auto [q, d] = out_at(t); return -0.5 * q.Y() * q.Y() * d.X(); });
  const double oval_length = span_sum(oval, 30, [&](double t) { gp_Pnt q; gp_Vec d; oval->D1(t, q, d); return d.Magnitude(); });
  CHECK(near(out_length, oval_length + 2.0 * kPi * offset, 1e-12));
  const occ::handle<Geom_OffsetSurface> side =
      new Geom_OffsetSurface(new Geom_SurfaceOfLinearExtrusion(oval, gp::DZ()), offset);
  const TopoDS_Face side_face =
      BRepBuilderAPI_MakeFace(side, oval->FirstParameter(), oval->LastParameter(), 0.0, h, 1e-7).Face();
  CHECK(BRepAdaptor_Surface(side_face).GetType() == GeomAbs_OffsetSurface);
  const GProp_GProps s = mitcad::analysis::surface_properties(side_face);
  CHECK(near(s.Mass(), out_length * h));
  CHECK(near_point(s.CentreOfMass(),
                   gp_Pnt(span_sum(oval, 30, [&](double t) { const auto [q, d] = out_at(t); return q.X() * d.Magnitude(); }) / out_length,
                          span_sum(oval, 30, [&](double t) { const auto [q, d] = out_at(t); return q.Y() * d.Magnitude(); }) / out_length,
                          h / 2.0),
                   100.0));
  gp_Trsf up;
  up.SetTranslation(gp_Vec(0.0, 0.0, h));
  const TopoDS_Face bottom = BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(out).Edge()).Wire()).Face();
  const TopoDS_Face top = TopoDS::Face(moved(bottom, up));
  const TopoDS_Compound closed = compound_of({side_face, facing(bottom, -gp::DZ()), facing(top, gp::DZ())});
  const GProp_GProps o = mitcad::analysis::volume_properties(closed);
  const double oval_area = span_sum(oval, 8, [&](double t) { gp_Pnt q; gp_Vec d; oval->D1(t, q, d); return q.X() * d.Y(); });
  CHECK(near(out_area, oval_area + oval_length * offset + kPi * offset * offset, 1e-12));
  CHECK(near(o.Mass(), out_area * h));
  CHECK(near(mitcad::analysis::surface_properties(closed).Mass(), out_length * h + 2.0 * out_area));
  CHECK(near_point(o.CentreOfMass(), gp_Pnt(out_mx / out_area, out_my / out_area, h / 2.0), 100.0));

  // A profile from the axis out to a cubic of 200 spans, turned about it by
  // 2 pi and by 1.5 pi: surfaces of revolution of 200 spans along, the
  // volume and centre by Pappus from the profile's spans.
  const int count = 201;
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, count);
  for (int i = 0; i < count; ++i) {
    const double t = static_cast<double>(i) / (count - 1);
    const double out_x = i % 2 == 0 ? 0.01 : -0.01;
    points->SetValue(i + 1, gp_Pnt(10.0 + 3.0 * std::sin(3.0 * kPi * t) + out_x, 0.0, 30.0 * t));
  }
  GeomAPI_Interpolate interpolate(points, false, 1e-9);
  interpolate.Perform();
  const occ::handle<Geom_BSplineCurve> meridian = interpolate.Curve();
  CHECK(meridian->NbKnots() > 200);
  const auto m_at = [&](double t) {
    gp_Pnt q;
    gp_Vec d;
    meridian->D1(t, q, d);
    return std::make_pair(q, d);
  };
  const double moment = span_sum(meridian, 8, [&](double t) { const auto [q, d] = m_at(t); return 0.5 * q.X() * q.X() * d.Z(); });
  const double square = span_sum(meridian, 8, [&](double t) { const auto [q, d] = m_at(t); return q.X() * q.X() * q.X() / 3.0 * d.Z(); });
  const double raised = span_sum(meridian, 8, [&](double t) { const auto [q, d] = m_at(t); return 0.5 * q.X() * q.X() * q.Z() * d.Z(); });
  const double flat = span_sum(meridian, 8, [&](double t) { const auto [q, d] = m_at(t); return q.X() * d.Z(); });
  const double swept = span_sum(meridian, 30, [&](double t) { const auto [q, d] = m_at(t); return q.X() * d.Magnitude(); });
  const gp_Pnt first = meridian->StartPoint();
  const gp_Pnt last = meridian->EndPoint();
  BRepBuilderAPI_MakeWire outline;
  outline.Add(BRepBuilderAPI_MakeEdge(meridian).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(last, gp_Pnt(0.0, 0.0, last.Z())).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(0.0, 0.0, last.Z()), gp::Origin()).Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(gp::Origin(), first).Edge());
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const double r0 = first.X();
  const double r1 = last.X();
  for (const double angle : {2.0 * kPi, 1.5 * kPi}) {
    const TopoDS_Shape turned_profile = BRepPrimAPI_MakeRevol(section, gp_Ax1(gp::Origin(), gp::DZ()), angle).Shape();
    const auto t = mitcad::analysis::physical_properties(turned_profile);
    const double volume = angle * moment;
    const bool whole = angle > 6.0;
    CHECK(near(t.volume, volume));
    CHECK(near(t.area, angle * swept + angle * (r0 * r0 + r1 * r1) / 2.0 + (whole ? 0.0 : 2.0 * flat)));
    CHECK(near_point(as_pnt(t.center_of_mass),
                     gp_Pnt(square * std::sin(angle) / volume, square * (1.0 - std::cos(angle)) / volume,
                            angle * raised / volume),
                     30.0));
  }

  // A cylinder of 5 mm scaled 2 times along x: rational B-spline patches
  // and curves around an ellipse of 10 by 5 mm.
  gp_GTrsf stretch;
  stretch.SetValue(1, 1, 2.0);
  const TopoDS_Shape elliptic = BRepBuilderAPI_GTransform(solid, stretch, true).Shape();
  double perimeter = 0.0;
  for (int i = 0; i < 16; ++i) {
    perimeter += gauss_sum(30, kPi * i / 8.0, kPi * (i + 1) / 8.0, [](double t) {
      return std::hypot(10.0 * std::sin(t), 5.0 * std::cos(t));
    });
  }
  const auto e = mitcad::analysis::physical_properties(elliptic);
  CHECK(near(e.volume, kPi * 10.0 * 5.0 * height));
  CHECK(near(e.area, 2.0 * kPi * 10.0 * 5.0 + perimeter * height));
  CHECK(near_vec(e.center_of_mass, 0.0, 0.0, height / 2.0));

  // A torus and a truncated cone, moved off the origin.
  gp_Trsf away;
  away.SetTranslation(gp_Vec(7.0, -3.0, 11.0));
  const auto torus = mitcad::analysis::physical_properties(moved(BRepPrimAPI_MakeTorus(20.0, 5.0).Shape(), away));
  CHECK(near(torus.volume, 2.0 * kPi * kPi * 20.0 * 25.0));
  CHECK(near(torus.area, 4.0 * kPi * kPi * 20.0 * 5.0));
  CHECK(near_vec(torus.center_of_mass, 7.0, -3.0, 11.0));
  const double base = 10.0;
  const double top_radius = 4.0;
  const double tall = 12.0;
  const auto cone =
      mitcad::analysis::physical_properties(moved(BRepPrimAPI_MakeCone(base, top_radius, tall).Shape(), away));
  const double sum = base * base + base * top_radius + top_radius * top_radius;
  CHECK(near(cone.volume, kPi * tall * sum / 3.0));
  CHECK(near(cone.area, kPi * (base + top_radius) * std::hypot(base - top_radius, tall) +
                            kPi * (base * base + top_radius * top_radius)));
  CHECK(near_vec(cone.center_of_mass, 7.0, -3.0,
                 11.0 + tall * (base * base + 2.0 * base * top_radius + 3.0 * top_radius * top_radius) / (4.0 * sum)));
}

void test_products_and_several_bodies() {
  // Two 2 mm cubes centred at (5, 5, 0) and (-5, -5, 0): Ixy = -sum x y m.
  const TopoDS_Shape c1 = box(4, 4, -1, 2, 2, 2);
  const TopoDS_Shape c2 = box(-6, -6, -1, 2, 2, 2);
  const auto p = mitcad::analysis::physical_properties({c1, c2});
  const double mc = 8e-6;
  CHECK(near(p.mass, 2.0 * mc));
  CHECK(near_vec(p.center_of_mass, 0.0, 0.0, 0.0));
  CHECK(near(p.inertia[0][1], -2.0 * mc * 25.0));
  CHECK(near(p.inertia[1][0], -2.0 * mc * 25.0));
  CHECK(std::abs(p.inertia[0][2]) < 1e-15);
  CHECK(near(p.inertia[0][0], 2.0 * mc * (8.0 / 12.0 + 25.0)));

  // Different densities move the centre of mass towards the heavier body.
  const auto w = mitcad::analysis::physical_properties({c1, c2}, {3.0, 1.0});
  CHECK(near(w.mass, 4.0 * mc));
  CHECK(near_vec(w.center_of_mass, 2.5, 2.5, 0.0));
  CHECK(near(w.volume, 16.0));
  // Zero density: no mass, but the centre of the volume.
  const auto z = mitcad::analysis::physical_properties(c1, 0.0);
  CHECK(z.mass == 0.0 && near_vec(z.center_of_mass, 5.0, 5.0, 0.0));
  CHECK(throws([&] { mitcad::analysis::physical_properties({c1, c2}, {1.0}); }));

  // An inside-out solid measures the same.
  const auto inverted = mitcad::analysis::physical_properties(c1.Reversed());
  CHECK(near(inverted.volume, 8.0));
  // A face has area but no volume; its centre is the area centre.
  const TopoDS_Face top = face_with_normal(c1, 0, 0, 1);
  const auto face = mitcad::analysis::physical_properties(top);
  CHECK(face.volume == 0.0 && face.mass == 0.0 && near(face.area, 4.0));
  CHECK(near_vec(face.center_of_mass, 5.0, 5.0, 1.0));
}

void test_distances() {
  using mitcad::analysis::min_distance;
  const TopoDS_Shape a = box(0, 0, 0, 10, 10, 10);
  const TopoDS_Shape b = box(15, 0, 0, 10, 10, 10);
  const auto d = min_distance(a, b);
  CHECK(near(d.value, 5.0) && !d.inside);
  CHECK(near(d.on_a.x, 10.0) && near(d.on_b.x, 15.0));

  const TopoDS_Shape vertex = BRepBuilderAPI_MakeVertex(gp_Pnt(3.0, 4.0, 22.0)).Vertex();
  const auto dv = min_distance(vertex, face_with_normal(a, 0, 0, 1));
  CHECK(near(dv.value, 12.0) && near_vec(dv.on_b, 3.0, 4.0, 10.0));

  // Skew lines.
  const TopoDS_Shape e1 = BRepBuilderAPI_MakeEdge(gp_Pnt(0, 0, 0), gp_Pnt(10, 0, 0)).Edge();
  const TopoDS_Shape e2 = BRepBuilderAPI_MakeEdge(gp_Pnt(5, -5, 3), gp_Pnt(5, 5, 3)).Edge();
  const auto de = min_distance(e1, e2);
  CHECK(near(de.value, 3.0) && near_vec(de.on_a, 5, 0, 0, 1e-7) && near_vec(de.on_b, 5, 0, 3, 1e-7));

  const auto inside = min_distance(box(2, 2, 2, 1, 1, 1), a);
  CHECK(inside.inside && near(inside.value, 0.0));
  CHECK(throws([&] { min_distance(TopoDS_Shape(), a); }));
}

void test_angles() {
  using mitcad::analysis::angle;
  const TopoDS_Shape a = box(0, 0, 0, 10, 10, 10);
  const TopoDS_Face top = face_with_normal(a, 0, 0, 1);
  const TopoDS_Face bottom = face_with_normal(a, 0, 0, -1);
  const TopoDS_Face side = face_with_normal(a, 1, 0, 0);
  CHECK(near(angle(top, side), kPi / 2.0));
  CHECK(near(angle(top, bottom), kPi));
  CHECK(near(angle(top, top), 0.0));

  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp::Origin(), gp::DZ()), kPi / 6.0);
  const TopoDS_Face turned = face_with_normal(moved(a, turn), std::cos(kPi / 6.0), std::sin(kPi / 6.0), 0.0);
  CHECK(!turned.IsNull() && near(angle(side, turned), kPi / 6.0));

  // Edges from a common vertex: the angle between them; otherwise acute.
  const gp_Pnt o(0, 0, 0);
  const TopoDS_Edge x_axis = BRepBuilderAPI_MakeEdge(o, gp_Pnt(1, 0, 0)).Edge();
  const TopoDS_Vertex start = TopExp::FirstVertex(x_axis);
  const TopoDS_Edge sixty =
      BRepBuilderAPI_MakeEdge(start, BRepBuilderAPI_MakeVertex(gp_Pnt(-0.5, std::sqrt(3.0) / 2.0, 0)))
          .Edge();
  CHECK(near(angle(x_axis, sixty), 2.0 * kPi / 3.0));
  const TopoDS_Edge diagonal = BRepBuilderAPI_MakeEdge(gp_Pnt(0, 1, 0), gp_Pnt(1, 2, 0)).Edge();
  const TopoDS_Edge backwards = BRepBuilderAPI_MakeEdge(gp_Pnt(1, 2, 0), gp_Pnt(0, 1, 0)).Edge();
  CHECK(near(angle(x_axis, diagonal), kPi / 4.0));
  CHECK(near(angle(x_axis, backwards), kPi / 4.0));
  // Edge against a face: angle to the plane.
  const TopoDS_Edge rising = BRepBuilderAPI_MakeEdge(gp_Pnt(0, 0, 20), gp_Pnt(1, 0, 21)).Edge();
  CHECK(near(angle(rising, top), kPi / 4.0));
  CHECK(near(angle(top, rising), kPi / 4.0));

  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(5.0, 20.0).Shape();
  CHECK(throws([&] { angle(face_of_type(cylinder, GeomAbs_Cylinder), top); }));
}

void test_lengths_and_circles() {
  using mitcad::analysis::circle;
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(5.0, 20.0).Shape();
  const TopoDS_Edge rim = circular_edge(cylinder);
  CHECK(near(mitcad::analysis::length(rim), 10.0 * kPi));
  const auto c = circle(rim);
  CHECK(c && near(c->radius, 5.0) && parallel(c->axis, 0, 0, 1) && near(c->sweep, 2.0 * kPi));
  CHECK(c && near(c->center.x, 0.0) && near(c->center.y, 0.0));

  const TopoDS_Edge arc =
      BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(1, 2, 3), gp::DX()), 4.0), 0.0, kPi / 2.0).Edge();
  const auto ca = circle(arc);
  CHECK(ca && near(ca->radius, 4.0) && near_vec(ca->center, 1, 2, 3) && near(ca->sweep, kPi / 2.0));
  CHECK(ca && parallel(ca->axis, 1, 0, 0));
  CHECK(near(mitcad::analysis::length(arc), 2.0 * kPi));

  const auto wall = circle(face_of_type(cylinder, GeomAbs_Cylinder));
  CHECK(wall && near(wall->radius, 5.0) && near_vec(wall->center, 0, 0, 10, 1e-7));
  const auto ball = circle(face_of_type(BRepPrimAPI_MakeSphere(gp_Pnt(1, 1, 1), 3.0).Shape(), GeomAbs_Sphere));
  CHECK(ball && near(ball->radius, 3.0) && near_vec(ball->center, 1, 1, 1));
  CHECK(!circle(face_of_type(cylinder, GeomAbs_Plane)));
  CHECK(!circle(BRepBuilderAPI_MakeEdge(gp_Pnt(0, 0, 0), gp_Pnt(1, 0, 0)).Edge()));

  const TopoDS_Shape a = box(0, 0, 0, 10, 20, 30);
  CHECK(near(mitcad::analysis::area(face_with_normal(a, 0, 0, 1)), 200.0));
  CHECK(near(mitcad::analysis::area(a), 2200.0));
  CHECK(near(mitcad::analysis::length(a), 4.0 * (10 + 20 + 30)));
  const TopoDS_Vertex corner = TopExp::FirstVertex(rim);
  const auto p = mitcad::analysis::point(corner);
  CHECK(near(std::hypot(p.x, p.y), 5.0));
  CHECK(throws([&] { mitcad::analysis::point(rim); }));
}

void test_interference() {
  const std::vector<TopoDS_Shape> bodies = {
      box(0, 0, 0, 10, 10, 10),  // 0
      box(5, 0, 0, 10, 10, 10),  // 1: overlaps 0 by 500
      box(40, 0, 0, 10, 10, 10), // 2: apart
      box(10, 0, 0, 10, 10, 10), // 3: touches 0, overlaps 1 by 500
      box(1, 1, 1, 2, 2, 2),     // 4: inside 0
  };
  const auto found = mitcad::analysis::interferences(bodies);
  CHECK(found.size() == 3);
  if (found.size() == 3) {
    CHECK(found[0].first == 0 && found[0].second == 1 && near(found[0].volume, 500.0, 1e-7));
    CHECK(found[1].first == 0 && found[1].second == 4 && near(found[1].volume, 8.0, 1e-7));
    CHECK(found[2].first == 1 && found[2].second == 3 && near(found[2].volume, 500.0, 1e-7));
    CHECK(near(mitcad::analysis::physical_properties(found[0].common).volume, 500.0, 1e-7));
  }
  mitcad::analysis::InterferenceOptions options;
  options.keep_shapes = false;
  options.min_volume = 10.0;
  const auto large = mitcad::analysis::interferences(bodies, options);
  CHECK(large.size() == 2 && large[0].common.IsNull());
}

void test_sections() {
  using mitcad::analysis::section;
  const TopoDS_Shape a = box(0, 0, 0, 10, 20, 30);
  const auto s = section(a, Plane{{0, 0, 10}, {0, 0, 1}});
  CHECK(s.edge_count == 4 && s.face_count == 1);
  CHECK(near(s.length, 60.0, 1e-7) && near(s.area, 200.0, 1e-7));

  const auto x = section(a, Plane{{5, 0, 0}, {2, 0, 0}});
  CHECK(near(x.area, 600.0, 1e-7));
  const auto miss = section(a, Plane{{0, 0, 100}, {0, 0, 1}});
  CHECK(miss.edge_count == 0 && miss.face_count == 0 && miss.area == 0.0);

  // A block with a hole: the cut face has the hole in it.
  const TopoDS_Shape hole =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(25, 15, -1), gp::DZ()), 6.0, 22.0).Shape();
  const TopoDS_Shape holed = BRepAlgoAPI_Cut(box(0, 0, 0, 50, 30, 20), hole).Shape();
  const auto h = section(holed, Plane{{0, 0, 10}, {0, 0, 1}});
  CHECK(h.face_count == 1 && near(h.area, 1500.0 - 36.0 * kPi, 1e-6));
  CHECK(near(h.length, 160.0 + 12.0 * kPi, 1e-6));

  const auto ball = section(BRepPrimAPI_MakeSphere(10.0).Shape(), Plane{{0, 0, 0}, {0, 1, 0}});
  CHECK(near(ball.area, 100.0 * kPi, 1e-6) && near(ball.length, 20.0 * kPi, 1e-6));

  // Clipping keeps the side behind the plane.
  using mitcad::analysis::clip;
  CHECK(near(mitcad::analysis::physical_properties(clip(a, Plane{{0, 0, 10}, {0, 0, 1}})).volume,
             2000.0, 1e-7));
  CHECK(near(mitcad::analysis::physical_properties(clip(a, Plane{{0, 0, 10}, {0, 0, -1}})).volume,
             4000.0, 1e-7));
  CHECK(near(mitcad::analysis::physical_properties(clip(a, Plane{{0, 0, -50}, {0, 0, 1}})).volume,
             0.0, 1e-7));
  CHECK(throws([&] { section(a, Plane{{0, 0, 0}, {0, 0, 0}}); }));
}

void test_compare() {
  using mitcad::analysis::compare;
  const TopoDS_Shape a = box(0, 0, 0, 10, 20, 30);

  const auto same = compare(a, BRepBuilderAPI_Copy(a).Shape());
  CHECK(near(same.volume_a, 6000.0) && near(same.volume_b, 6000.0));
  CHECK(same.a_minus_b && *same.a_minus_b < 1e-6 && same.b_minus_a && *same.b_minus_a < 1e-6);
  CHECK(same.relative_difference && *same.relative_difference < 1e-9);
  CHECK(same.max_deviation < 1e-7 && same.a_to_b.samples > 1000 && same.b_to_a.samples > 1000);
  CHECK(same.bounds_difference < 1e-9);

  // Same geometry, different topology (side faces split in two) and
  // orientation.
  const TopoDS_Shape halves =
      BRepAlgoAPI_Fuse(box(0, 0, 0, 10, 20, 15), box(0, 0, 15, 10, 20, 15)).Shape();
  const auto split = compare(a, halves.Reversed());
  CHECK(near(split.volume_b, 6000.0, 1e-9));
  CHECK(split.relative_difference && *split.relative_difference < 1e-9);
  CHECK(split.max_deviation < 1e-7);

  // Shifted by 0.1 mm.
  gp_Trsf shift;
  shift.SetTranslation(gp_Vec(0.1, 0.0, 0.0));
  const auto shifted = compare(a, moved(a, shift));
  CHECK(shifted.a_minus_b && near(*shifted.a_minus_b, 60.0, 1e-6));
  CHECK(shifted.b_minus_a && near(*shifted.b_minus_a, 60.0, 1e-6));
  CHECK(shifted.relative_difference && near(*shifted.relative_difference, 0.02, 1e-6));
  CHECK(near(shifted.max_deviation, 0.1, 1e-7));
  CHECK(shifted.a_to_b.rms > 0.0 && shifted.a_to_b.rms < 0.1);
  CHECK(near(shifted.bounds_difference, 0.1, 1e-9));

  // One vertical edge rounded by 2 mm.
  BRepFilletAPI_MakeFillet fillet(a);
  for (TopExp_Explorer it(a, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const gp_Pnt p0 = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt p1 = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (p0.X() == 0.0 && p1.X() == 0.0 && p0.Y() == 0.0 && p1.Y() == 0.0) {
      fillet.Add(2.0, edge);
    }
  }
  const auto rounded = compare(a, fillet.Shape());
  CHECK(rounded.a_minus_b && near(*rounded.a_minus_b, (1.0 - kPi / 4.0) * 4.0 * 30.0, 1e-6));
  CHECK(rounded.b_minus_a && *rounded.b_minus_a < 1e-6);
  // The sharp edge is 2 (sqrt 2 - 1) from the fillet; the fillet's middle
  // is 2 (1 - 1/sqrt 2) inside the box faces.
  CHECK(near(rounded.a_to_b.max, 2.0 * (std::sqrt(2.0) - 1.0), 1e-6));
  CHECK(near(rounded.b_to_a.max, 2.0 * (1.0 - 1.0 / std::sqrt(2.0)), 1e-3));
  CHECK(near(rounded.max_deviation, rounded.a_to_b.max));
  CHECK(near(rounded.a_to_b.at.x, 0.0) && near(rounded.a_to_b.at.y, 0.0));
  CHECK(rounded.bounds_difference < 1e-9);

  mitcad::analysis::CompareOptions one_way;
  one_way.symmetric = false;
  one_way.samples = 100;
  const auto quick = compare(a, fillet.Shape(), one_way);
  CHECK(quick.b_to_a.samples == 0 && quick.a_to_b.samples > 100);

  // Nearly coincident shapes: the booleans are left out when no sample
  // lies farther than asked (mitcad#69), not otherwise.
  mitcad::analysis::CompareOptions close;
  close.booleans_above = 1e-3;
  const auto near_same = compare(a, BRepBuilderAPI_Copy(a).Shape(), close);
  CHECK(!near_same.a_minus_b && !near_same.b_minus_a && !near_same.relative_difference);
  CHECK(near_same.max_deviation < 1e-7 && near_same.b_to_a.samples > 1000);
  const auto apart = compare(a, moved(a, shift), close);
  CHECK(apart.relative_difference && near(*apart.relative_difference, 0.02, 1e-6));
  // Booleans that took nearly coincident shapes as apart are unknown; the
  // shift's and the rounding's differences are within what their
  // deviations allow.
  using mitcad::analysis::plausible_differences;
  const double area = 2.0 * 2.0 * (200.0 + 300.0 + 600.0);
  CHECK(plausible_differences(120.0, 0.1, area, 1e-4));
  CHECK(plausible_differences(*rounded.a_minus_b, rounded.max_deviation, area, 1e-4));
  CHECK(!plausible_differences(12000.0, 1e-5, area, 1e-4));
}

} // namespace

int main() {
  try {
    test_box_properties();
    test_round_properties();
    test_spline_bounded_properties();
    test_span_by_span_properties();
    test_products_and_several_bodies();
    test_distances();
    test_angles();
    test_lengths_and_circles();
    test_interference();
    test_sections();
    test_compare();
  } catch (const std::exception& error) {
    std::fprintf(stderr, "unexpected exception: %s\n", error.what());
    return 1;
  }
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("all geometry analysis tests passed\n");
  return 0;
}
