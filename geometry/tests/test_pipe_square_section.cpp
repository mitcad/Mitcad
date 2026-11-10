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
// whatever its edges' tolerance. And (mitcad#1559) mitred corners between
// curved pieces: the section square to each piece; (mitcad#1564) rounded
// ones too, the section turned about each corner.

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
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
#include <Geom_Ellipse.hxx>
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
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
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
    const gp_Ax2 axes(c.center, c.normal, c.x_axis);
    const occ::handle<Geom_Curve> conic =
        c.major - c.minor > 1e-12 ? occ::handle<Geom_Curve>(new Geom_Ellipse(gp_Elips(axes, c.major, c.minor)))
                                  : occ::handle<Geom_Curve>(new Geom_Circle(gp_Circ(axes, c.major)));
    return occ::handle<Geom_Curve>(new Geom_TrimmedCurve(conic, c.first, c.last));
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

// The piece run the other way.
ModelCurve reversed(const ModelCurve& c) {
  ModelCurve r = c;
  if (c.kind == ModelCurveKind::Line) {
    std::swap(r.start, r.end);
    return r;
  }
  if (c.kind == ModelCurveKind::Conic) {
    // About the opposite normal, the angles negated.
    r.normal = c.normal.Reversed();
    r.first = -c.last;
    r.last = -c.first;
    return r;
  }
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
// tangent to it at its ends; with `kink` (degrees) each piece after the
// first starts turned from the wave's slope by that much about Y, so that
// the pieces meet at that angle.
Path sine_wave(int pieces, double kink = 0.0) {
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
    gp_Vec start = slope(from);
    if (piece > 0) {
      start.Rotate(gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 1.0, 0.0)), kink * kPi / 180.0);
    }
    interpolate.Load(start, slope(to));
    interpolate.Perform();
    CHECK(interpolate.IsDone());
    const TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(occ::handle<Geom_Curve>(interpolate.Curve())).Edge();
    path.push_back({"c" + std::to_string(piece + 1), edge_curve(edge)});
  }
  return path;
}

// The sine wave drawn as `count` lines between its points.
Path sine_lines(int count) {
  Path path;
  const auto z = [](double x) { return 15.0 * std::sin(2.0 * kPi * x / 100.0); };
  for (int i = 0; i < count; ++i) {
    const double a = 100.0 * i / count;
    const double b = 100.0 * (i + 1) / count;
    PathCurve piece;
    piece.name = "c" + std::to_string(i + 1);
    piece.curve.kind = ModelCurveKind::Line;
    piece.curve.start = gp_Pnt(a, 0.0, z(a));
    piece.curve.end = gp_Pnt(b, 0.0, z(b));
    path.push_back(piece);
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

constexpr int kIsoPoints = 48;

// The points of a face's iso line across the path at `t`, as iso_cut cuts
// it.
std::vector<gp_Pnt> iso_points(const occ::handle<Geom_Surface>& surface, double u0, double u1, double v0, double v1,
                               bool across_u, double t) {
  std::vector<gp_Pnt> points;
  for (int k = 0; k <= kIsoPoints; ++k) {
    const double w = static_cast<double>(k) / kIsoPoints;
    points.push_back(across_u ? surface->Value(u0 + (u1 - u0) * w, v0 + (v1 - v0) * t)
                              : surface->Value(u0 + (u1 - u0) * t, v0 + (v1 - v0) * w));
  }
  return points;
}

Cut iso_cut(const occ::handle<Geom_Surface>& surface, double u0, double u1, double v0, double v1, bool across_u,
            double t) {
  constexpr int count = kIsoPoints;
  const std::vector<gp_Pnt> points = iso_points(surface, u0, u1, v0, v1, across_u, t);
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

// A side face of the pipe: its surface, its bounds, and whether the
// section runs along u (the parameter whose iso line is an arc of the
// section's circle), with the centres of 65 cuts along it. A face that
// turns the section about a corner of the path has the cuts one way all
// centred on the corner (`corner`, along u when `corner_across_u`).
struct Side {
  occ::handle<Geom_Surface> surface;
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  bool across_u = true;
  std::vector<gp_Pnt> centres;
  bool corner = false;
  bool corner_across_u = true;
};

constexpr int kSamples = 64;

Cut cut(const Side& side, double t) {
  return iso_cut(side.surface, side.u0, side.u1, side.v0, side.v1, side.across_u, t);
}

// Whether the cuts of the face along u (or v) are all centred on one point.
bool centred_on_a_point(const Side& side, bool across_u) {
  const gp_Pnt first = iso_cut(side.surface, side.u0, side.u1, side.v0, side.v1, across_u, 0.0).center;
  for (int k = 1; k <= 4; ++k) {
    const gp_Pnt next = iso_cut(side.surface, side.u0, side.u1, side.v0, side.v1, across_u, k / 4.0).center;
    // (Not where the cuts are straight: their centres are not finite.)
    if (!(next.Distance(first) <= 1e-3)) {
      return false;
    }
  }
  return true;
}

Side side_of(const TopoDS_Face& face) {
  Side side;
  BRepTools::UVBounds(face, side.u0, side.u1, side.v0, side.v1);
  side.surface = BRep_Tool::Surface(face);
  const Cut across_u = iso_cut(side.surface, side.u0, side.u1, side.v0, side.v1, true, 0.5);
  const Cut across_v = iso_cut(side.surface, side.u0, side.u1, side.v0, side.v1, false, 0.5);
  side.across_u = std::abs(across_u.radius - kSize / 2.0) < std::abs(across_v.radius - kSize / 2.0);
  for (int k = 0; k <= kSamples; ++k) {
    side.centres.push_back(cut(side, static_cast<double>(k) / kSamples).center);
  }
  // (A turn's cuts across the corner and about its axis can both be arcs
  // of the section's radius.)
  if (side.centres.front().Distance(side.centres.back()) <= 1e-3) {
    side.corner = true;
    side.corner_across_u = side.across_u;
  } else if (centred_on_a_point(side, !side.across_u)) {
    side.corner = true;
    side.corner_across_u = !side.across_u;
  }
  return side;
}

// The angle (radians) a face turning the section about a corner spans
// round the section: its middle cut's length over its radius.
double turn_span(const Side& side) {
  const std::vector<gp_Pnt> points =
      iso_points(side.surface, side.u0, side.u1, side.v0, side.v1, side.corner_across_u, 0.5);
  const Cut c = iso_cut(side.surface, side.u0, side.u1, side.v0, side.v1, side.corner_across_u, 0.5);
  double length = 0.0;
  for (std::size_t k = 0; k + 1 < points.size(); ++k) {
    length += points[k].Distance(points[k + 1]);
  }
  return length / c.radius;
}

// The largest angle (degrees) between a cut across the pipe's side and the
// plane square to the path at the cut's centre, over 20 cuts whose centres
// are spread evenly along the path; `off` the centres' largest distance
// from the path. Faces round a corner of the path (all their cuts one way
// centred on the corner) are not cut.
double worst_angle(const Shape& shape, const Measured& m, double& off) {
  std::vector<Side> sides;
  for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane) {
      Side side = side_of(face);
      if (!side.corner && side.centres.front().Distance(side.centres.back()) > 1e-3) {
        sides.push_back(std::move(side));
      }
    }
  }
  CHECK(!sides.empty());
  double worst = 0.0;
  off = 0.0;
  for (int i = 0; i < kStations; ++i) {
    const gp_Pnt target = m.point((i + 0.5) / kStations * m.length);
    // The cut centred nearest to the station: the best of 65 along each
    // face, then narrowed down.
    const Side* best = nullptr;
    double at = 0.0;
    double nearest = std::numeric_limits<double>::infinity();
    for (const Side& side : sides) {
      for (int k = 0; k <= kSamples; ++k) {
        const double d = side.centres[static_cast<std::size_t>(k)].Distance(target);
        if (d < nearest) {
          nearest = d;
          best = &side;
          at = static_cast<double>(k) / kSamples;
        }
      }
    }
    if (best == nullptr) {
      continue;
    }
    double a = std::max(0.0, at - 1.0 / kSamples);
    double b = std::min(1.0, at + 1.0 / kSamples);
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
void check_crossing(const char* what, const Path& wave, double sagitta, int arc_pieces) {
  const std::vector<ModelCurve> pieces =
      intersection_curves(wave, gp_Dir(0.0, 1.0, 0.0), arc(sagitta, arc_pieces), gp_Dir(0.0, 0.0, 1.0));
  CHECK(!pieces.empty());
  if (!pieces.empty()) {
    check_square(what, chained(pieces, gp_Pnt(0.0, 0.0, 0.0)));
  }
}

void check_crossing(const char* what, double sagitta, int sine_pieces, int arc_pieces) {
  check_crossing(what, sine_wave(sine_pieces), sagitta, arc_pieces);
}

// What a rounded corner adds to a pipe of a circle of radius `r` holding
// the circle's area times its path's length, where the path turns by
// `angle` (radians): the legs end square at the corner and overlap inside
// it by two wedges of tan(angle / 2) times the half circle's first moment
// about the corner's axis (square to both legs), 2 r^3 / 3, and the half
// circle outside turns about that axis, filling `angle` times its moment
// (Pappus). (A quarter turn: (pi / 2 - 2) 2 r^3 / 3, as geometry.pipe_corners
// has it.)
double rounded_corner(double r, double angle) {
  return (angle - 2.0 * std::tan(angle / 2.0)) * 2.0 * r * r * r / 3.0;
}

// (mitcad#1564) OCCT's pipe corners say what they do on the standard
// error while MITCAD_TRACE_PIPE_CORNERS is set (port patch 1666).
void trace_pipe_corners(bool on) {
#ifdef _WIN32
  _putenv_s("MITCAD_TRACE_PIPE_CORNERS", on ? "1" : "");
#else
  if (on) {
    setenv("MITCAD_TRACE_PIPE_CORNERS", "1", 1);
  } else {
    unsetenv("MITCAD_TRACE_PIPE_CORNERS");
  }
#endif
}

// (mitcad#1564) Checks the 8 mm pipe with rounded corners along the
// crossing of `wave` and an arc bending `sagitta`: the section turned
// about each corner of the path from 1e-2 rad on (port patch 1662) by
// faces whose cuts one way are all centred on the corner, spanning half
// the circle round it (outside the corner; the legs meet inside it), and
// square to the path within 0.01 degrees at 20 points elsewhere; the pipe
// holds the circle's area times its length and what each corner adds
// within 0.1 %.
void check_rounded(const char* what, const Path& wave, double sagitta) {
  const std::vector<ModelCurve> pieces =
      intersection_curves(wave, gp_Dir(0.0, 1.0, 0.0), arc(sagitta, 1), gp_Dir(0.0, 0.0, 1.0));
  CHECK(!pieces.empty());
  if (pieces.empty()) {
    return;
  }
  const Path path = chained(pieces, gp_Pnt(0.0, 0.0, 0.0));
  const Measured m = measured(path);
  double expected = kPi * kSize * kSize / 4.0 * m.length;
  int corners = 0;
  for (std::size_t i = 0; i + 1 < m.curves.size(); ++i) {
    gp_Pnt p;
    gp_Vec before;
    gp_Vec after;
    m.curves[i]->D1(m.curves[i]->LastParameter(), p, before);
    m.curves[i + 1]->D1(m.curves[i + 1]->FirstParameter(), p, after);
    // (A joint under 1e-2 rad, where the pieces go on tangentially, as
    // the sine's two middle lines do, is no corner.)
    const double angle = before.Angle(after);
    std::printf("  joint %zu: %.6f degrees%s\n", i + 1, angle * 180.0 / kPi, angle >= 1e-2 ? ", a corner" : "");
    if (angle >= 1e-2) {
      ++corners;
      expected += rounded_corner(kSize / 2.0, angle);
    }
  }
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = path;
  spec.size = kSize;
  spec.corners = PipeCorners::Rounded;
  // Each try of the pipe's corners (rounded, rounded from the halved
  // section, mitred, the section turned through) traced by OCCT: what the
  // trim of each corner did, where the round's cut was refused, and the
  // turns built.
  std::printf("  the corners' trace (standard error):\n");
  std::fflush(stdout);
  trace_pipe_corners(true);
  const ShapePtr result = pipe(spec);
  trace_pipe_corners(false);
  std::fflush(stderr);
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  int turns = 0;
  double span = 0.0;
  for (TopExp_Explorer f(result->occt(), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane) {
      const Side side = side_of(face);
      if (side.corner) {
        ++turns;
        span += turn_span(side);
      }
    }
  }
  double off = 0.0;
  const double angle = worst_angle(*result, m, off);
  const double held = volume(*result);
  std::printf(" %s, rounded: %zu piece(s), %d corner(s), %d turning face(s) spanning %.6f half circles, path %.6f "
              "mm, worst %.6f degrees from square, centres off the path %.2e mm, volume %.6f of %.6f (%+.5f %%)\n",
              what, path.size(), corners, turns, span / kPi, m.length, angle, off, held, expected,
              100.0 * (held - expected) / expected);
  CHECK(corners > 0);
  CHECK(turns >= corners);
  CHECK_NEAR_TOL(span, kPi * corners, 1e-2);
  CHECK(angle <= kMaxAngle);
  CHECK(off <= 1e-3);
  CHECK_NEAR_TOL(held, expected, 1e-3);
}

void test_bent_path() { check_crossing("bent", 20.0, 1, 1); }

void test_nearly_planar_path() { check_crossing("nearly planar", 0.3, 1, 1); }

// The same from chains of pieces joined tangentially: the crossing comes
// in four pieces.
void test_bent_path_in_pieces() { check_crossing("bent, in pieces", 20.0, 2, 3); }

void test_nearly_planar_path_in_pieces() { check_crossing("nearly planar, in pieces", 0.3, 2, 3); }

// (mitcad#1559, port patches 1661 and 1662) Pieces of a path meeting at
// corners, mitred as a pipe's corners are by default: the sine wave drawn
// as 4 lines crossed with the bent arc (elliptic arcs meeting at about 60
// degrees where the lines turn), and the sine wave of two splines meeting
// at a kink of 1 degree, crossed with either arc. Each side of a corner
// is swept along a curve that is not a line or a circle, so the two
// sides' seams do not cross each other.
void test_four_lines() { check_crossing("4 lines", sine_lines(4), 20.0, 1); }

void test_kink() {
  check_crossing("kink of 1 degree", sine_wave(2, 1.0), 20.0, 1);
  check_crossing("kink of 1 degree, nearly planar", sine_wave(2, 1.0), 0.3, 1);
}

// (mitcad#1559, port patches 1663 and 1664) Corners whose sides'
// intersection comes in pieces, mitred along the bisector plane: the sine
// wave drawn as 10 lines, and kinks of 3 degrees (with the nearly planar
// arc), 0.3 degrees (the bent arc) and 0.05 degrees (the nearly planar
// arc).
void test_ten_lines() { check_crossing("10 lines", sine_lines(10), 20.0, 1); }

void test_small_kinks() {
  check_crossing("kink of 3 degrees, nearly planar", sine_wave(2, 3.0), 0.3, 1);
  check_crossing("kink of 0.3 degrees", sine_wave(2, 0.3), 20.0, 1);
  check_crossing("kink of 0.05 degrees, nearly planar", sine_wave(2, 0.05), 0.3, 1);
}

// (mitcad#1564, port patch 1666) The same corners rounded, where the two
// sides' intersection breaks up: the sine wave drawn as 10 lines crossed
// with the bent arc, and the kink of 3 degrees with the nearly planar arc.
void test_rounded_ten_lines() { check_rounded("10 lines", sine_lines(10), 20.0); }

void test_rounded_small_kink() { check_rounded("kink of 3 degrees, nearly planar", sine_wave(2, 3.0), 0.3); }

// The arc through three points (less than half a turn).
PathCurve arc3(const gp_Pnt& a, const gp_Pnt& b, const gp_Pnt& c) {
  const gp_Vec ab(a, b);
  const gp_Vec ac(a, c);
  const gp_Vec n = ab.Crossed(ac);
  const gp_Pnt center =
      a.Translated((n.Crossed(ab) * ac.SquareMagnitude() + ac.Crossed(n) * ab.SquareMagnitude()) /
                   (2.0 * n.SquareMagnitude()));
  PathCurve piece;
  piece.name = "arc";
  ModelCurve& m = piece.curve;
  m.kind = ModelCurveKind::Conic;
  m.center = center;
  m.normal = gp_Dir(n);
  m.x_axis = gp_Dir(gp_Vec(center, a));
  m.major = center.Distance(a);
  m.minor = m.major;
  m.first = 0.0;
  m.last = gp_Vec(center, a).AngleWithRef(gp_Vec(center, c), gp_Vec(n));
  return piece;
}

PathCurve line3(const gp_Pnt& a, const gp_Pnt& b) {
  PathCurve piece;
  piece.name = "line";
  piece.curve.kind = ModelCurveKind::Line;
  piece.curve.start = a;
  piece.curve.end = b;
  return piece;
}

// (mitcad#1559) A closed stirrup of lines and arcs (an IFC file's
// reinforcing bar, a 12 mm swept disk): its arcs meet its lines
// tangentially, but two of them in a plane turned by 0.12 degrees (a kink
// that is a mitred corner since port patch 1662), and its closing line
// meets the last arc at 73 degrees and the first line at 107. Mitred, the
// corner of the arc's torus and the closing line's cylinder is not built
// (their intersection does not close, their cuts by the bisector plane lie
// 0.31 mm apart): the pipe is rounded there, as before 1662, and a valid
// solid holding the circle's area times the path's length within 0.2 %
// (the rounded corners hold 0.11 % less). With 1662's threshold for
// rounded corners as well, their turns at the slight kinks did not close
// (an invalid solid), and the intersection of their sides overran an array
// (port patch 1665).
void test_stirrup() {
  const std::vector<gp_Pnt> p{
      {-69.0, 0.0, -122.0},
      {-69.0, 0.0, -79.0},
      {-54.9411254969544, 0.0, -45.0588745030457},
      {-21.0000000000001, 0.0, -31.0},
      {21.0, 0.0, -31.0},
      {54.9411254969543, 0.0, -45.0588745030456},
      {69.0, 0.0, -78.9999999999999},
      {69.0, 0.00000000000000089, -321.0},
      {54.993978595716, 1.21791490472038, -354.941125496954},
      {21.1804517666064, 4.1582215855126, -369.0},
      {-20.6616529376114, 7.79666547283599, -369.0},
      {-54.4751797667207, 10.7369721536282, -354.941125496954},
      {-68.4812011710042, 11.9548870583485, -320.999999999999},
      {-69.0, 12.0, -79.0},
      {-54.9411254969544, 12.0, -45.0588745030457},
      {-21.0000000000001, 12.0, -31.0},
      {21.0, 12.0, -31.0},
      {54.9411254969543, 12.0, -45.0588745030456},
      {69.0, 12.0, -78.9999999999999}};
  Path path;
  for (std::size_t i = 0; i + 1 < p.size(); i += 3) {
    path.push_back(line3(p[i], p[i + 1]));
    if (i + 3 < p.size()) {
      path.push_back(arc3(p[i + 1], p[i + 2], p[i + 3]));
    }
  }
  path.push_back(line3(p.back(), p.front()));
  const Measured m = measured(path);
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = path;
  spec.size = 12.0;
  const ShapePtr result = pipe(spec);
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  const double expected = kPi * 36.0 * m.length;
  const double held = volume(*result);
  std::printf(" stirrup: %zu pieces, path %.6f mm, volume %.6f of %.6f (%+.5f %%)\n", path.size(), m.length, held,
              expected, 100.0 * (held - expected) / expected);
  CHECK_NEAR_TOL(held, expected, 2e-3);
}

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
  test::guarded("corners of 4 lines", test::test_four_lines);
  test::guarded("kink", test::test_kink);
  test::guarded("corners of 10 lines", test::test_ten_lines);
  test::guarded("small kinks", test::test_small_kinks);
  test::guarded("rounded corners of 10 lines", test::test_rounded_ten_lines);
  test::guarded("rounded kink", test::test_rounded_small_kink);
  test::guarded("stirrup", test::test_stirrup);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("pipe square section: all checks passed\n");
  return 0;
}
