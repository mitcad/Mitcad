// SPDX-License-Identifier: MIT
// A pipe's section stays square to its path (mitcad#1555): an 8 mm circle
// swept along the intersection curve of two sketches, a sine wave on XZ
// and an arc on XY, which is a smooth curve that does not lie in a plane;
// once with an arc that bends 20 mm across the path (plainly not planar)
// and once with one that bends 0.3 mm over its 100 mm (out of its plane by
// a fraction of a millimetre), each also from chains of curves joined
// tangentially (the crossing in pieces), and along a helix. The pipe's
// side is cut across at 20 points along the path: each cut, a circle of
// the side's surface, lies in the plane square to the path's tangent at
// its centre within 0.01 degrees, and the pipe holds the circle's area
// times the path's length within 0.1 %. A path is planar (swept with its
// plane's normal as a constant binormal) by its own points and tangents,
// whatever its edges' tolerance.

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <limits>
#include <optional>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRep_Builder.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepLib.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GC_MakeSegment.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Circle.hxx>
#include <Geom_Curve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "../src/spine.hpp"
#include "check.hpp"
#include "mitcad/geometry/intersection_curve.hpp"
#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/sketch_ref.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kSize = 8.0;       // the circle's diameter
constexpr double kMaxAngle = 0.01;  // degrees
constexpr int kStations = 20;

// The OCCT curve of a crossing piece or a sketch curve.
occ::handle<Geom_Curve> geom_curve(const ModelCurve& c) {
  if (c.kind == ModelCurveKind::Line) {
    return occ::handle<Geom_Curve>(GC_MakeSegment(c.start, c.end).Value());
  }
  if (c.kind == ModelCurveKind::Helix) {
    // A line on the unrolled cylinder, its 3D curve within 1e-9 mm.
    const occ::handle<Geom_Surface> surface =
        new Geom_CylindricalSurface(gp_Ax3(c.center, c.normal, c.x_axis), c.major);
    const gp_Vec2d direction(1.0, c.minor / (2.0 * kPi));
    const occ::handle<Geom2d_Curve> line =
        new Geom2d_Line(gp_Pnt2d(c.first, c.minor * c.first / (2.0 * kPi)), gp_Dir2d(direction));
    TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(line, surface, 0.0, (c.last - c.first) * direction.Magnitude()).Edge();
    BRepLib::BuildCurves3d(edge, 1.0e-9, GeomAbs_C2, 14, 256);
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    return occ::handle<Geom_Curve>(new Geom_TrimmedCurve(curve, first, last));
  }
  if (c.kind == ModelCurveKind::Conic) {
    const occ::handle<Geom_Curve> circle = new Geom_Circle(gp_Circ(gp_Ax2(c.center, c.normal, c.x_axis), c.major));
    return occ::handle<Geom_Curve>(new Geom_TrimmedCurve(circle, c.first, c.last));
  }
  std::vector<double> distinct;
  std::vector<int> mults;
  for (const double k : c.knots) {
    if (!distinct.empty() && std::abs(k - distinct.back()) <= 1e-12) {
      ++mults.back();
    } else {
      distinct.push_back(k);
      mults.push_back(1);
    }
  }
  NCollection_Array1<gp_Pnt> poles(1, static_cast<int>(c.poles.size()));
  for (std::size_t i = 0; i < c.poles.size(); ++i) {
    poles.SetValue(static_cast<int>(i) + 1, c.poles[i]);
  }
  NCollection_Array1<double> knots(1, static_cast<int>(distinct.size()));
  NCollection_Array1<int> multiplicities(1, static_cast<int>(distinct.size()));
  for (std::size_t i = 0; i < distinct.size(); ++i) {
    knots.SetValue(static_cast<int>(i) + 1, distinct[i]);
    multiplicities.SetValue(static_cast<int>(i) + 1, mults[i]);
  }
  if (c.weights.empty()) {
    return occ::handle<Geom_Curve>(new Geom_BSplineCurve(poles, knots, multiplicities, c.degree));
  }
  NCollection_Array1<double> weights(1, static_cast<int>(c.weights.size()));
  for (std::size_t i = 0; i < c.weights.size(); ++i) {
    weights.SetValue(static_cast<int>(i) + 1, c.weights[i]);
  }
  return occ::handle<Geom_Curve>(new Geom_BSplineCurve(poles, weights, knots, multiplicities, c.degree));
}

// The piece run the other way (B-splines only, which the crossing gives).
ModelCurve reversed(const ModelCurve& c) {
  ModelCurve r = c;
  std::reverse(r.poles.begin(), r.poles.end());
  std::reverse(r.weights.begin(), r.weights.end());
  const double sum = c.knots.front() + c.knots.back();
  r.knots.clear();
  for (auto k = c.knots.rbegin(); k != c.knots.rend(); ++k) {
    r.knots.push_back(sum - *k);
  }
  return r;
}

gp_Pnt start_of(const ModelCurve& c) {
  const occ::handle<Geom_Curve> g = geom_curve(c);
  return g->Value(g->FirstParameter());
}

gp_Pnt end_of(const ModelCurve& c) {
  const occ::handle<Geom_Curve> g = geom_curve(c);
  return g->Value(g->LastParameter());
}

// The pieces end to end, from the end nearest to `from`, as the model
// joins them into one curve.
Path chained(std::vector<ModelCurve> pieces, const gp_Pnt& from) {
  Path path;
  gp_Pnt at = from;
  bool first = true;
  while (!pieces.empty()) {
    std::size_t best = 0;
    bool flip = false;
    double distance = std::numeric_limits<double>::infinity();
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      const double d0 = start_of(pieces[i]).Distance(at);
      const double d1 = end_of(pieces[i]).Distance(at);
      if (d0 < distance) {
        distance = d0;
        best = i;
        flip = false;
      }
      if (d1 < distance) {
        distance = d1;
        best = i;
        flip = true;
      }
    }
    CHECK(first || distance <= 1e-6);
    first = false;
    const ModelCurve piece = flip ? reversed(pieces[best]) : pieces[best];
    path.push_back({"curve" + std::to_string(path.size() + 1), piece});
    at = end_of(piece);
    pieces.erase(pieces.begin() + static_cast<std::ptrdiff_t>(best));
  }
  return path;
}

// The sine wave z = 15 sin(2 pi x / 100) on XZ, x from 0 to 100, as
// `pieces` sketch splines end to end, each through 21 of its points and
// tangent to it at its ends.
Path sine_wave(int pieces) {
  Path path;
  constexpr int count = 20;
  const auto z = [](double x) { return 15.0 * std::sin(2.0 * kPi * x / 100.0); };
  const auto slope = [](double x) {
    return gp_Vec(1.0, 0.0, 15.0 * 2.0 * kPi / 100.0 * std::cos(2.0 * kPi * x / 100.0));
  };
  for (int piece = 0; piece < pieces; ++piece) {
    const double from = 100.0 * piece / pieces;
    const double to = 100.0 * (piece + 1) / pieces;
    const occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, count + 1);
    for (int i = 0; i <= count; ++i) {
      const double x = from + (to - from) * i / count;
      points->SetValue(i + 1, gp_Pnt(x, 0.0, z(x)));
    }
    GeomAPI_Interpolate interpolate(points, false, 1e-9);
    interpolate.Load(slope(from), slope(to));
    interpolate.Perform();
    CHECK(interpolate.IsDone());
    const TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(occ::handle<Geom_Curve>(interpolate.Curve())).Edge();
    path.push_back({"c" + std::to_string(piece + 1), edge_curve(edge)});
  }
  return path;
}

// An arc on XY bending `sagitta` across x from 0 to 100 (its middle at
// (50, sagitta)), from x = -15 to 115, as `pieces` arcs end to end.
Path arc(double sagitta, int pieces) {
  const double radius = 2500.0 / (2.0 * sagitta) + sagitta / 2.0;
  const double first = std::acos(65.0 / radius);
  const double last = std::acos(-65.0 / radius);
  Path path;
  for (int i = 0; i < pieces; ++i) {
    PathCurve piece;
    piece.name = "c" + std::to_string(i + 1);
    ModelCurve& c = piece.curve;
    c.kind = ModelCurveKind::Conic;
    c.center = gp_Pnt(50.0, sagitta - radius, 0.0);
    c.normal = gp_Dir(0.0, 0.0, 1.0);
    c.x_axis = gp_Dir(1.0, 0.0, 0.0);
    c.major = radius;
    c.minor = radius;
    c.first = first + (last - first) * i / pieces;
    c.last = first + (last - first) * (i + 1) / pieces;
    path.push_back(piece);
  }
  return path;
}

// The pieces' curves, to measure the path along.
struct Measured {
  std::vector<occ::handle<Geom_Curve>> curves;
  std::vector<double> lengths;
  double length = 0.0;

  // The point at arc length `s` along the path.
  gp_Pnt point(double s) const {
    for (std::size_t i = 0; i < curves.size(); ++i) {
      if (s <= lengths[i] || i + 1 == curves.size()) {
        const GeomAdaptor_Curve curve(curves[i]);
        const double along = std::min(s, lengths[i]);
        return curves[i]->Value(GCPnts_AbscissaPoint(curve, along, curve.FirstParameter()).Parameter());
      }
      s -= lengths[i];
    }
    return gp_Pnt();
  }
};

Measured measured(const Path& path) {
  Measured m;
  for (const PathCurve& piece : path) {
    const occ::handle<Geom_Curve> g = geom_curve(piece.curve);
    m.curves.push_back(g);
    m.lengths.push_back(GCPnts_AbscissaPoint::Length(GeomAdaptor_Curve(g)));
    m.length += m.lengths.back();
  }
  return m;
}

// The unit tangent of the path at its point nearest to `p`.
gp_Dir tangent_near(const Measured& m, const gp_Pnt& p, double& distance) {
  distance = std::numeric_limits<double>::infinity();
  gp_Dir tangent(1.0, 0.0, 0.0);
  for (const occ::handle<Geom_Curve>& g : m.curves) {
    GeomAPI_ProjectPointOnCurve project(p, g);
    for (int i = 1; i <= project.NbPoints(); ++i) {
      if (project.Distance(i) < distance) {
        distance = project.Distance(i);
        gp_Pnt q;
        gp_Vec v;
        g->D1(project.Parameter(i), q, v);
        tangent = gp_Dir(v);
      }
    }
  }
  return tangent;
}

// The centre of the circle through three points.
gp_Pnt circumcenter(const gp_Pnt& a, const gp_Pnt& b, const gp_Pnt& c) {
  const gp_Vec ab(a, b);
  const gp_Vec ac(a, c);
  const gp_Vec n = ab.Crossed(ac);
  const gp_Vec to = (n.Crossed(ab) * ac.SquareMagnitude() + ac.Crossed(n) * ab.SquareMagnitude()) /
                    (2.0 * n.SquareMagnitude());
  return a.Translated(to);
}

// A cut across a side face of the pipe: the face's iso line across the
// path at `t` (0 to 1) along it (an arc of the section's circle, or all of
// it), its centre and its plane's normal.
struct Cut {
  gp_Pnt center;
  gp_Vec normal;
  double radius = 0.0;
};

Cut iso_cut(const occ::handle<Geom_Surface>& surface, double u0, double u1, double v0, double v1, bool across_u,
            double t) {
  constexpr int count = 48;
  std::vector<gp_Pnt> points;
  for (int k = 0; k <= count; ++k) {
    const double w = static_cast<double>(k) / count;
    points.push_back(across_u ? surface->Value(u0 + (u1 - u0) * w, v0 + (v1 - v0) * t)
                              : surface->Value(u0 + (u1 - u0) * t, v0 + (v1 - v0) * w));
  }
  Cut c;
  c.center = circumcenter(points[0], points[count / 3], points[2 * count / 3]);
  c.radius = c.center.Distance(points[0]);
  // Newell's normal of the fan from the centre through the points.
  std::vector<gp_Pnt> loop{c.center};
  loop.insert(loop.end(), points.begin(), points.end());
  c.normal = gp_Vec(0.0, 0.0, 0.0);
  for (std::size_t k = 0; k < loop.size(); ++k) {
    const gp_Pnt& a = loop[k];
    const gp_Pnt& b = loop[(k + 1) % loop.size()];
    c.normal += gp_Vec((a.Y() - b.Y()) * (a.Z() + b.Z()), (a.Z() - b.Z()) * (a.X() + b.X()),
                       (a.X() - b.X()) * (a.Y() + b.Y()));
  }
  return c;
}

Cut cut(const TopoDS_Face& face, double t) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  // The section runs along the parameter whose iso line is an arc of the
  // section's circle.
  const Cut across_u = iso_cut(surface, u0, u1, v0, v1, true, 0.5);
  const Cut across_v = iso_cut(surface, u0, u1, v0, v1, false, 0.5);
  const bool u = std::abs(across_u.radius - kSize / 2.0) < std::abs(across_v.radius - kSize / 2.0);
  return iso_cut(surface, u0, u1, v0, v1, u, t);
}

// The largest angle (degrees) between a cut across the pipe's side and the
// plane square to the path at the cut's centre, over 20 cuts whose centres
// are spread evenly along the path; `off` the centres' largest distance
// from the path. Faces round a corner of the path (all their cuts centred
// on the corner) are not cut.
double worst_angle(const Shape& shape, const Measured& m, double& off) {
  std::vector<TopoDS_Face> sides;
  for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane &&
        cut(face, 0.0).center.Distance(cut(face, 1.0).center) > 1e-3) {
      sides.push_back(face);
    }
  }
  CHECK(!sides.empty());
  double worst = 0.0;
  off = 0.0;
  for (int i = 0; i < kStations; ++i) {
    const gp_Pnt target = m.point((i + 0.5) / kStations * m.length);
    // The cut centred nearest to the station: the best of 64 along each
    // face, then narrowed down.
    constexpr int samples = 64;
    const TopoDS_Face* best = nullptr;
    double at = 0.0;
    double nearest = std::numeric_limits<double>::infinity();
    for (const TopoDS_Face& face : sides) {
      for (int k = 0; k <= samples; ++k) {
        const double d = cut(face, static_cast<double>(k) / samples).center.Distance(target);
        if (d < nearest) {
          nearest = d;
          best = &face;
          at = static_cast<double>(k) / samples;
        }
      }
    }
    if (best == nullptr) {
      continue;
    }
    double a = std::max(0.0, at - 1.0 / samples);
    double b = std::min(1.0, at + 1.0 / samples);
    const double ratio = (std::sqrt(5.0) - 1.0) / 2.0;
    for (int k = 0; k < 60; ++k) {
      const double c = b - ratio * (b - a);
      const double d = a + ratio * (b - a);
      if (cut(*best, c).center.Distance(target) < cut(*best, d).center.Distance(target)) {
        b = d;
      } else {
        a = c;
      }
    }
    const Cut found = cut(*best, (a + b) / 2.0);
    double distance = 0.0;
    const gp_Dir tangent = tangent_near(m, found.center, distance);
    off = std::max(off, distance);
    const double angle = gp_Dir(found.normal).Angle(tangent) * 180.0 / kPi;
    worst = std::max(worst, std::min(angle, 180.0 - angle));
    std::printf("  cut %2d: centre (%.4f, %.4f, %.4f), %.2e mm from the station, off the path %.2e mm, %.6f "
                "degrees from square\n",
                i, found.center.X(), found.center.Y(), found.center.Z(), found.center.Distance(target), distance,
                std::min(angle, 180.0 - angle));
  }
  return worst;
}

// Checks the 8 mm pipe along `path`: square to it within 0.01 degrees at
// 20 points, holding the circle's area times its length within 0.1 %.
void check_square(const char* what, const Path& path) {
  const Measured m = measured(path);
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = path;
  spec.size = kSize;
  const ShapePtr result = pipe(spec);
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  double off = 0.0;
  const double angle = worst_angle(*result, m, off);
  const double expected = kPi * kSize * kSize / 4.0 * m.length;
  const double held = volume(*result);
  std::printf(" %s: %zu piece(s), path %.6f mm, worst %.6f degrees from square, centres off the path %.2e mm, "
              "volume %.6f of %.6f (%+.5f %%)\n",
              what, path.size(), m.length, angle, off, held, expected, 100.0 * (held - expected) / expected);
  CHECK(angle <= kMaxAngle);
  CHECK(off <= 1e-3);
  CHECK_NEAR_TOL(held, expected, 1e-3);
}

// The pipe along the intersection curve of the sine wave (in
// `sine_pieces`) and an arc bending `sagitta` (in `arc_pieces`).
void check_crossing(const char* what, double sagitta, int sine_pieces, int arc_pieces) {
  const std::vector<ModelCurve> pieces =
      intersection_curves(sine_wave(sine_pieces), gp_Dir(0.0, 1.0, 0.0), arc(sagitta, arc_pieces),
                          gp_Dir(0.0, 0.0, 1.0));
  CHECK(!pieces.empty());
  if (!pieces.empty()) {
    check_square(what, chained(pieces, gp_Pnt(0.0, 0.0, 0.0)));
  }
}

void test_bent_path() { check_crossing("bent", 20.0, 1, 1); }

void test_nearly_planar_path() { check_crossing("nearly planar", 0.3, 1, 1); }

// The same from chains of pieces joined tangentially: the crossing comes
// in four pieces.
void test_bent_path_in_pieces() { check_crossing("bent, in pieces", 20.0, 2, 3); }

void test_nearly_planar_path_in_pieces() { check_crossing("nearly planar, in pieces", 0.3, 2, 3); }

// Along a helix of radius 20 rising 15 a turn, two turns (swept with its
// axis as a constant binormal: the section keeps its angle to the axis,
// square to the path all along it as at its start).
void test_helix_path() {
  PathCurve helix;
  helix.name = "helix";
  helix.curve.kind = ModelCurveKind::Helix;
  helix.curve.center = gp_Pnt(0.0, 0.0, 0.0);
  helix.curve.normal = gp_Dir(0.0, 0.0, 1.0);
  helix.curve.x_axis = gp_Dir(1.0, 0.0, 0.0);
  helix.curve.major = 20.0;
  helix.curve.minor = 15.0;
  helix.curve.first = 0.0;
  helix.curve.last = 4.0 * kPi;
  check_square("helix", Path{helix});
}

// The wire of the path's curves, each edge given `tolerance`.
TopoDS_Wire loose_wire(const Path& path, double tolerance) {
  BRepBuilderAPI_MakeWire wire;
  for (const PathCurve& piece : path) {
    const TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(geom_curve(piece.curve)).Edge();
    BRep_Builder().UpdateEdge(edge, tolerance);
    wire.Add(edge);
  }
  return wire.Wire();
}

// A path is planar by its own points and tangents, not by its edges'
// tolerance: the nearly planar crossing (0.3 mm out of its plane) with
// edges of 0.5 mm tolerance has no plane; the sine wave on XZ, with the
// same tolerance, lies in XZ.
void test_plane_by_points() {
  const std::vector<ModelCurve> pieces =
      intersection_curves(sine_wave(1), gp_Dir(0.0, 1.0, 0.0), arc(0.3, 1), gp_Dir(0.0, 0.0, 1.0));
  CHECK(!pieces.empty());
  if (pieces.empty()) {
    return;
  }
  const mitcad::geometry::detail::Spine crossing(loose_wire(chained(pieces, gp_Pnt(0.0, 0.0, 0.0)), 0.5));
  CHECK(!crossing.plane_normal());
  const mitcad::geometry::detail::Spine sine(loose_wire(sine_wave(2), 0.5));
  const std::optional<gp_Dir> normal = sine.plane_normal();
  CHECK(normal && normal->IsParallel(gp_Dir(0.0, 1.0, 0.0), 1e-9));
}

} // namespace
} // namespace test

int main() {
  test::guarded("bent path", test::test_bent_path);
  test::guarded("nearly planar path", test::test_nearly_planar_path);
  test::guarded("bent path in pieces", test::test_bent_path_in_pieces);
  test::guarded("nearly planar path in pieces", test::test_nearly_planar_path_in_pieces);
  test::guarded("helix path", test::test_helix_path);
  test::guarded("plane by points", test::test_plane_by_points);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("pipe square section: all checks passed\n");
  return 0;
}
