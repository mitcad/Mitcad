// SPDX-License-Identifier: MIT
#include "pipe_folds.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>
#include <limits>
#include <map>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepAlgoAPI_Check.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_HArray1.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <NCollection_List.hxx>
#include <ShapeFix_Face.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>
#include <gp_Ax1.hxx>
#include <gp_Mat.hxx>
#include <gp_XYZ.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "history.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kPi = 3.14159265358979323846;
constexpr double kTwoPi = 2.0 * kPi;

// A corner that turns back further than this (radians) has no miter that
// can be built.
constexpr double kTurnBack = kPi * 179.0 / 180.0;

// Points sampled on each edge of a profile's loops.
constexpr int kProfileSamples = 32;

// Points sampled on each curve of the path that is not a circular arc,
// for its bends.
constexpr int kBendSamples = 32;

// A place along the spine: an edge and the length along it from its start.
struct Station {
  int edge = 0;
  double local = 0.0;
};

// A joint of two edges of the spine: where it lies, the edges before and
// after it, the directions arriving and leaving, and how far it turns.
struct Joint {
  double s = 0.0;
  int before = 0;
  int after = 0;
  gp_Dir in;
  gp_Dir out;
  double turn = 0.0;
};

// The loops moved by `move` (copies; the names follow their edges).
std::vector<NamedWire> placed(const std::vector<NamedWire>& loops, const gp_Trsf& move) {
  std::vector<NamedWire> out;
  for (const NamedWire& loop : loops) {
    BRepBuilderAPI_Transform transform(loop.wire, move, true);
    NamedWire moved;
    moved.wire = TopoDS::Wire(transform.Shape());
    for (const auto& [edge, name] : loop.edges) {
      moved.edges.emplace_back(TopoDS::Edge(transform.ModifiedShape(edge)), name);
    }
    out.push_back(std::move(moved));
  }
  return out;
}

// A valid shape that encloses a volume (mitcad#385: folded sides can pass
// the check and enclose a negative volume).
bool sound(const Shape& shape) {
  if (!is_valid(shape.occt())) {
    return false;
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.Mass() > 0.0;
}

// Whether OCCT's checker finds faces of the shape crossing each other.
bool self_interfering(const Shape& shape) {
  BRepAlgoAPI_Check check(shape.occt(), false, true);
  return !check.IsValid();
}

// The face the loops bound (the outer one first, then its holes).
TopoDS_Face profile_face(const std::vector<NamedWire>& loops) {
  BRepBuilderAPI_MakeFace outer(loops.front().wire, true);
  if (!outer.IsDone()) {
    throw std::runtime_error("the profile does not bound a planar face");
  }
  TopoDS_Face face = outer.Face();
  if (loops.size() > 1) {
    BRepBuilderAPI_MakeFace holed(face);
    for (std::size_t l = 1; l < loops.size(); ++l) {
      holed.Add(loops[l].wire);
    }
    if (!holed.IsDone()) {
      throw std::runtime_error("the profile's holes could not be added to its face");
    }
    ShapeFix_Face fix(holed.Face());
    fix.FixOrientation();
    face = fix.Face();
  }
  return face;
}

// The profile the loops bound turned about the axis by `angle` (radians,
// counter-clockwise about it): the profile cut along the axis's line, each
// side turned on its own (a side that reaches past the axis turns through
// it, so the profile turned whole folds), as several solids that overlap or
// touch. Sides named after the loops' edges, caps `first` and `last`.
std::vector<ShapePtr> turned(const std::vector<NamedWire>& loops, const gp_Ax1& axis, double angle,
                             const NameList& first, const NameList& last, const char* what) {
  const TopoDS_Face face = profile_face(loops);
  Bnd_Box box;
  BRepBndLib::Add(face, box);
  const double size = std::sqrt(box.SquareExtent()) + 1.0;
  const gp_Pnt low = box.CornerMin();
  const gp_Pnt high = box.CornerMax();
  const gp_Pnt middle((low.XYZ() + high.XYZ()) / 2.0);
  const gp_Vec along(axis.Direction());
  const gp_Pnt foot = axis.Location().Translated(along * gp_Vec(axis.Location(), middle).Dot(along));
  const TopoDS_Edge line =
      BRepBuilderAPI_MakeEdge(foot.Translated(along * -size), foot.Translated(along * size)).Edge();
  BRepAlgoAPI_Splitter splitter;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(face);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(line);
  splitter.SetArguments(arguments);
  splitter.SetTools(tools);
  splitter.SetNonDestructive(true);
  build(splitter);
  if (!splitter.IsDone() || splitter.HasErrors()) {
    throw std::runtime_error(std::string(what) + ": the profile could not be cut along the bend's axis");
  }
  const bool whole_turn = angle >= kTwoPi - 1.0e-9;
  std::vector<ShapePtr> out;
  for (TopExp_Explorer f(splitter.Shape(), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Shape side = f.Current();
    BRepPrimAPI_MakeRevol revol(side, axis, whole_turn ? kTwoPi : angle, true);
    if (!revol.IsDone()) {
      throw std::runtime_error(std::string(what) + ": a side of the profile could not be turned");
    }
    FaceNamer namer(oriented_solid(revol.Shape(), what));
    ShapeMap edges;
    TopExp::MapShapes(side, TopAbs_EDGE, edges);
    for (const NamedWire& loop : loops) {
      for (const auto& [edge, name] : loop.edges) {
        if (splitter.IsDeleted(edge)) {
          continue;
        }
        NCollection_List<TopoDS_Shape> images = splitter.Modified(edge);
        if (images.IsEmpty()) {
          images.Append(edge);
        }
        for (const TopoDS_Shape& image : images) {
          if (edges.Contains(image)) {
            namer.generated(revol, image, name);
          }
        }
      }
    }
    if (!whole_turn) {
      name_caps(namer, revol.FirstShape(), first);
      name_caps(namer, revol.LastShape(), last);
    }
    out.push_back(namer.shape());
  }
  return out;
}

// The region beyond the plane square to `in` through the corner and before
// the plane square to `out` (a wedge whose edge runs through the corner
// along in x out), within `size` of the corner.
TopoDS_Shape corner_wedge(const gp_Pnt& corner, const gp_Dir& in, const gp_Dir& out, double size) {
  const gp_Dir normal(gp_Vec(in).Crossed(gp_Vec(out)));
  gp_Vec e1 = gp_Vec(normal).Crossed(gp_Vec(in));
  if (e1.Dot(gp_Vec(out)) > 0.0) {
    e1.Reverse();
  }
  gp_Vec e2 = gp_Vec(normal).Crossed(gp_Vec(out));
  if (e2.Dot(gp_Vec(in)) < 0.0) {
    e2.Reverse();
  }
  const gp_Vec middle = (gp_Vec(in) - gp_Vec(out)).Normalized();
  const gp_Pnt base = corner.Translated(gp_Vec(normal) * -size);
  BRepBuilderAPI_MakePolygon polygon;
  polygon.Add(base);
  polygon.Add(base.Translated(e1 * size));
  polygon.Add(base.Translated((e1 + middle) * size));
  polygon.Add(base.Translated((e2 + middle) * size));
  polygon.Add(base.Translated(e2 * size));
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(normal) * (2.0 * size)).Shape();
}

// (mitcad#1708) Whether every loop of the profiles is a circle about the
// path's point `centre` square to its tangent there: then how the sweep
// turns the profile about the path does not change what it sweeps.
bool round_profiles(const std::vector<SweptProfile>& profiles, const gp_Pnt& centre, const gp_Dir& tangent,
                    double tolerance) {
  for (const SweptProfile& profile : profiles) {
    for (const NamedWire& loop : profile.loops) {
      int edges = 0;
      for (TopExp_Explorer e(loop.wire, TopAbs_EDGE); e.More(); e.Next()) {
        ++edges;
        const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
        if (curve.GetType() != GeomAbs_Circle) {
          return false;
        }
        const gp_Circ circle = curve.Circle();
        if (circle.Location().Distance(centre) > tolerance ||
            std::abs(circle.Axis().Direction().Dot(tangent)) < 1.0 - 1.0e-9) {
          return false;
        }
      }
      if (edges != 1) {
        return false;
      }
    }
  }
  return !profiles.empty();
}

// (mitcad#1709) A tight bend of a curve whose bend changes along it (an
// ellipse, a spline) in a path in one plane, under a round profile. Along
// such a curve the centre of the bend moves across the profile, so no cut
// of the profile keeps its sides from folding. The solid is built from its
// boundary instead: points within the profile's radius of the path, so the
// tube the profile's rim sweeps, less the part of it that lies inside: past
// the crease where the tube meets itself, which runs through the points as
// far from two points of the curve on either side of its tightest point as
// the rim reaches. The surface through the centres of the bend lies inside
// the solid (each point is held by the profile at the point of the path
// nearest to it).

// A curve of a path in one plane, along the wire from its start (the
// parameter `l` from 0 to span()).
class PlaneCurve {
public:
  PlaneCurve(const Spine::EdgeCurve& edge, const gp_Dir& binormal) : m_edge(edge), m_binormal(binormal) {}

  double span() const { return m_edge.last - m_edge.first; }
  const gp_Dir& binormal() const { return m_binormal; }

  void derivatives(double l, gp_Pnt& p, gp_Vec& d1, gp_Vec& d2) const {
    const double t = m_edge.forward ? m_edge.first + l : m_edge.last - l;
    m_edge.curve->D2(t, p, d1, d2);
    if (!m_edge.forward) {
      d1.Reverse();
    }
  }

  gp_Pnt point(double l) const {
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(l, p, d1, d2);
    return p;
  }

  gp_Dir tangent(double l) const {
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(l, p, d1, d2);
    return gp_Dir(d1);
  }

  // The curvature, positive where the curve turns towards binormal x
  // tangent.
  double curvature(double l) const {
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(l, p, d1, d2);
    const double speed = d1.Magnitude();
    return d1.Crossed(d2).Dot(gp_Vec(m_binormal)) / (speed * speed * speed);
  }

  // The unit normal towards the side the curve turns to (`side` the sign
  // of its curvature).
  gp_Dir inward(double l, double side) const {
    return gp_Dir(gp_Vec(m_binormal).Crossed(gp_Vec(tangent(l))) * side);
  }

private:
  Spine::EdgeCurve m_edge;
  gp_Dir m_binormal;
};

// Where the tube of radius `radius` about a curve meets itself: the curve's
// tightest point `lv` (radius of its bend `bend`, the angle `top` from the
// inward normal at which the rim reaches the centre of that bend), and
// samples along the crease, at q from 0 (the rim's point at `top` over the
// tightest point) to 1 (the crease's deepest point, in the curve's plane):
// at the angle top (1 - q^2) from the inward normal, the parameters before
// (l1) and after (l2) the tightest point whose rims meet there.
struct Crease {
  double side = 1.0;
  double lv = 0.0;
  double bend = 0.0;
  double top = 0.0;
  std::vector<double> q;
  std::vector<double> l1;
  std::vector<double> l2;

  double angle(std::size_t k) const { return top * (1.0 - q[k] * q[k]); }
};

// Samples along a crease.
constexpr int kCreaseSamples = 32;

// The crease of the tube of radius `radius` about the curve, where it has
// one tightest point that the rim reaches past the centre of, and the
// crease lies within the curve: the curve turns one way, its bend's radius
// falls to the tightest point and grows after it, more than `radius` at
// both ends. None otherwise.
std::optional<Crease> crease_of(const PlaneCurve& curve, double radius) {
  const double span = curve.span();
  constexpr int samples = 128;
  std::vector<double> bend(samples + 1);
  double side = 0.0;
  for (int k = 0; k <= samples; ++k) {
    const double kappa = curve.curvature(span * k / samples);
    if (std::abs(kappa) < 1.0e-12 || (side != 0.0 && (kappa > 0.0) != (side > 0.0))) {
      return std::nullopt;
    }
    side = kappa > 0.0 ? 1.0 : -1.0;
    bend[static_cast<std::size_t>(k)] = 1.0 / std::abs(kappa);
  }
  const auto tightest = static_cast<int>(std::min_element(bend.begin(), bend.end()) - bend.begin());
  if (tightest == 0 || tightest == samples) {
    return std::nullopt;
  }
  for (int k = 0; k < samples; ++k) {
    const double before = bend[static_cast<std::size_t>(k)];
    const double after = bend[static_cast<std::size_t>(k) + 1];
    if (k < tightest ? !(after < before) : !(after > before)) {
      return std::nullopt;
    }
  }
  if (!(bend.front() > radius * (1.0 + 1.0e-9)) || !(bend.back() > radius * (1.0 + 1.0e-9))) {
    return std::nullopt;
  }
  const auto bend_at = [&](double l) { return 1.0 / std::abs(curve.curvature(l)); };
  double lo = span * (tightest - 1) / samples;
  double hi = span * (tightest + 1) / samples;
  for (int i = 0; i < 100; ++i) {
    const double m1 = lo + (hi - lo) / 3.0;
    const double m2 = hi - (hi - lo) / 3.0;
    if (bend_at(m1) < bend_at(m2)) {
      hi = m2;
    } else {
      lo = m1;
    }
  }
  Crease crease;
  crease.side = side;
  crease.lv = (lo + hi) / 2.0;
  crease.bend = bend_at(crease.lv);
  if (!(crease.bend < radius * (1.0 - 1.0e-9))) {
    return std::nullopt;
  }
  crease.top = std::acos(crease.bend / radius);
  const double lv = crease.lv;
  const gp_Dir e1 = curve.tangent(lv);
  const gp_Dir e2 = curve.inward(lv, side);
  const double h = 1.0e-4 * span;
  const double second = (bend_at(lv + h) - 2.0 * crease.bend + bend_at(lv - h)) / (h * h);
  if (!(second > 0.0)) {
    return std::nullopt;
  }
  const double tolerance = 1.0e-13 * std::max(1.0, span);
  double l1 = lv;
  double l2 = lv;
  crease.q.push_back(0.0);
  crease.l1.push_back(lv);
  crease.l2.push_back(lv);
  for (int k = 1; k <= kCreaseSamples; ++k) {
    const double q = static_cast<double>(k) / kCreaseSamples;
    const double w = radius * std::cos(crease.top * (1.0 - q * q));
    // Where the rim's offsets at w before and after the tightest point
    // meet, in the plane.
    const auto gap = [&](double a, double b) {
      const gp_Vec d(curve.point(b).Translated(gp_Vec(curve.inward(b, side)) * w),
                     curve.point(a).Translated(gp_Vec(curve.inward(a, side)) * w));
      return std::make_pair(d.Dot(gp_Vec(e1)), d.Dot(gp_Vec(e2)));
    };
    if (k == 1) {
      // The offset's swallowtail: its cusps where the bend's radius is w,
      // its crossing beyond them; for a start, the crossing of a bend
      // symmetric about its tightest point.
      const double cusp = std::sqrt(2.0 * (w - crease.bend) / second);
      double a = 1.001 * cusp;
      double b = 3.0 * cusp;
      const double at_a = gap(lv - a, lv + a).first;
      if ((at_a > 0.0) == (gap(lv - b, lv + b).first > 0.0)) {
        return std::nullopt;
      }
      for (int i = 0; i < 100; ++i) {
        const double m = (a + b) / 2.0;
        ((gap(lv - m, lv + m).first > 0.0) == (at_a > 0.0) ? a : b) = m;
      }
      l1 = lv - (a + b) / 2.0;
      l2 = lv + (a + b) / 2.0;
    } else {
      // The crease leaves the tightest point in proportion to q.
      const double grow = q / (static_cast<double>(k - 1) / kCreaseSamples);
      l1 = lv + (l1 - lv) * grow;
      l2 = lv + (l2 - lv) * grow;
    }
    bool converged = false;
    for (int it = 0; it < 60 && !converged; ++it) {
      const auto [f1, f2] = gap(l1, l2);
      if (std::hypot(f1, f2) < 1.0e-13 * std::max(1.0, radius)) {
        converged = true;
        break;
      }
      const double d = 1.0e-7 * std::max(1.0, span);
      const auto [a1, a2] = gap(l1 + d, l2);
      const auto [b1, b2] = gap(l1 - d, l2);
      const auto [c1, c2] = gap(l1, l2 + d);
      const auto [g1, g2] = gap(l1, l2 - d);
      const double j11 = (a1 - b1) / (2.0 * d);
      const double j21 = (a2 - b2) / (2.0 * d);
      const double j12 = (c1 - g1) / (2.0 * d);
      const double j22 = (c2 - g2) / (2.0 * d);
      const double det = j11 * j22 - j12 * j21;
      if (std::abs(det) < 1.0e-300) {
        return std::nullopt;
      }
      const double step1 = (f1 * j22 - f2 * j12) / det;
      const double step2 = (j11 * f2 - j21 * f1) / det;
      l1 -= step1;
      l2 -= step2;
      converged = std::abs(step1) + std::abs(step2) < tolerance;
    }
    const auto [r1, r2] = gap(l1, l2);
    if (!converged || std::hypot(r1, r2) > 1.0e-9 * std::max(1.0, radius) || !(l1 > 1.0e-9 * span) ||
        !(l1 < lv) || !(l2 > lv) || !(l2 < span * (1.0 - 1.0e-9))) {
      return std::nullopt;
    }
    crease.q.push_back(q);
    crease.l1.push_back(l1);
    crease.l2.push_back(l2);
  }
  return crease;
}

// The surface through a grid of points (rows along u, columns along v),
// cubic, through each point at its row's and its column's fraction of the
// grid.
occ::handle<Geom_BSplineSurface> through_grid(const NCollection_Array2<gp_Pnt>& points) {
  const int nu = points.ColLength();
  const int nv = points.RowLength();
  const auto fractions = [](int n) {
    occ::handle<NCollection_HArray1<double>> out = new NCollection_HArray1<double>(1, n);
    for (int i = 1; i <= n; ++i) {
      out->SetValue(i, static_cast<double>(i - 1) / (n - 1));
    }
    return out;
  };
  const occ::handle<NCollection_HArray1<double>> us = fractions(nu);
  const occ::handle<NCollection_HArray1<double>> vs = fractions(nv);
  const auto through = [](const occ::handle<NCollection_HArray1<gp_Pnt>>& pts,
                          const occ::handle<NCollection_HArray1<double>>& params) {
    GeomAPI_Interpolate interpolate(pts, params, false, 1.0e-12);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error("the tube's surface could not be interpolated");
    }
    return interpolate.Curve();
  };
  // Along u for each column, then along v through the poles.
  std::vector<occ::handle<Geom_BSplineCurve>> columns;
  for (int j = 1; j <= nv; ++j) {
    occ::handle<NCollection_HArray1<gp_Pnt>> pts = new NCollection_HArray1<gp_Pnt>(1, nu);
    for (int i = 1; i <= nu; ++i) {
      pts->SetValue(i, points(i, j));
    }
    columns.push_back(through(pts, us));
  }
  const int np = columns.front()->NbPoles();
  std::vector<occ::handle<Geom_BSplineCurve>> rows;
  for (int k = 1; k <= np; ++k) {
    occ::handle<NCollection_HArray1<gp_Pnt>> pts = new NCollection_HArray1<gp_Pnt>(1, nv);
    for (int j = 1; j <= nv; ++j) {
      pts->SetValue(j, columns[static_cast<std::size_t>(j) - 1]->Pole(k));
    }
    rows.push_back(through(pts, vs));
  }
  const int nq = rows.front()->NbPoles();
  NCollection_Array2<gp_Pnt> poles(1, np, 1, nq);
  for (int k = 1; k <= np; ++k) {
    for (int m = 1; m <= nq; ++m) {
      poles.SetValue(k, m, rows[static_cast<std::size_t>(k) - 1]->Pole(m));
    }
  }
  const occ::handle<Geom_BSplineCurve>& u = columns.front();
  const occ::handle<Geom_BSplineCurve>& v = rows.front();
  return new Geom_BSplineSurface(poles, u->Knots(), v->Knots(), u->Multiplicities(), v->Multiplicities(), u->Degree(),
                                 v->Degree());
}

// Points across the tube between two angles from the inward normal, and
// along it.
constexpr int kTubeAcross = 33;
constexpr int kTubeAlong = 41;

// Half of the tube of radius `radius` about the curve from `from` to `to`
// (`half` +1: the half towards the binormal): across it an exact half
// circle (rational: its rim at the ends is the circle itself), along it
// through the circle's poles at kTubeAlong points.
occ::handle<Geom_BSplineSurface> half_tube(const PlaneCurve& curve, double side, double radius, double from, double to,
                                           double half) {
  const double across[5] = {1.0, 1.0, 0.0, -1.0, -1.0};
  const double up[5] = {0.0, 1.0, 1.0, 1.0, 0.0};
  const double w = std::sqrt(2.0) / 2.0;
  const double weights[5] = {1.0, w, 1.0, w, 1.0};
  occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, kTubeAlong);
  for (int i = 1; i <= kTubeAlong; ++i) {
    params->SetValue(i, static_cast<double>(i - 1) / (kTubeAlong - 1));
  }
  std::vector<occ::handle<Geom_BSplineCurve>> columns;
  for (int j = 0; j < 5; ++j) {
    occ::handle<NCollection_HArray1<gp_Pnt>> pts = new NCollection_HArray1<gp_Pnt>(1, kTubeAlong);
    for (int i = 1; i <= kTubeAlong; ++i) {
      const double l = from + (to - from) * params->Value(i);
      pts->SetValue(i, curve.point(l).Translated(gp_Vec(curve.inward(l, side)) * (radius * across[j]) +
                                                 gp_Vec(curve.binormal()) * (half * radius * up[j])));
    }
    GeomAPI_Interpolate interpolate(pts, params, false, 1.0e-12);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error("the tube's surface could not be interpolated");
    }
    columns.push_back(interpolate.Curve());
  }
  const occ::handle<Geom_BSplineCurve>& along = columns.front();
  NCollection_Array2<gp_Pnt> poles(1, along->NbPoles(), 1, 5);
  NCollection_Array2<double> pole_weights(1, along->NbPoles(), 1, 5);
  for (int k = 1; k <= along->NbPoles(); ++k) {
    for (int j = 0; j < 5; ++j) {
      poles.SetValue(k, j + 1, columns[static_cast<std::size_t>(j)]->Pole(k));
      pole_weights.SetValue(k, j + 1, weights[j]);
    }
  }
  NCollection_Array1<double> knots(1, 3);
  NCollection_Array1<int> mults(1, 3);
  knots.SetValue(1, 0.0);
  knots.SetValue(2, 0.5);
  knots.SetValue(3, 1.0);
  mults.SetValue(1, 3);
  mults.SetValue(2, 2);
  mults.SetValue(3, 3);
  return new Geom_BSplineSurface(poles, pole_weights, along->Knots(), knots, along->Multiplicities(), mults,
                                 along->Degree(), 2);
}

// Adds to `sewing` the faces of the tube of radius `radius` about the curve,
// up to where it meets itself along `crease`, each side of the tightest
// point and of the curve's plane on its own (each face a whole surface, none
// folds): near the curve's ends the whole half tube, exact across (its rims
// at the ends the circles themselves); nearer the tightest point the tube
// above the crease's top and, below it, the tube from there up to the
// crease.
void add_creased_faces(BRepBuilderAPI_Sewing& sewing, const PlaneCurve& curve, const Crease& crease, double radius,
                       double tolerance, const char* what) {
  const double span = curve.span();
  const gp_Vec up(curve.binormal());
  const auto rim = [&](double l, double angle, double half) {
    return curve.point(l).Translated(gp_Vec(curve.inward(l, crease.side)) * (radius * std::cos(angle)) +
                                     up * (half * radius * std::sin(angle)));
  };
  // Where the whole half tubes end: half way to the crease's ends.
  const double before_end = crease.l1.back() / 2.0;
  const double after_start = crease.l2.back() + (span - crease.l2.back()) / 2.0;
  const int nq = static_cast<int>(crease.q.size());
  const auto add = [&](const occ::handle<Geom_Surface>& surface) {
    BRepBuilderAPI_MakeFace face(surface, tolerance);
    if (!face.IsDone()) {
      throw std::runtime_error(std::string(what) + ": the tube of a tight bend could not be built");
    }
    sewing.Add(face.Face());
  };
  for (const double half : {1.0, -1.0}) {
    add(half_tube(curve, crease.side, radius, 0.0, before_end, half));
    add(half_tube(curve, crease.side, radius, after_start, span, half));
    for (const bool before : {true, false}) {
      const double outer = before ? before_end : after_start;
      // Above the crease's top: between there and the tightest point.
      NCollection_Array2<gp_Pnt> above(1, kTubeAlong, 1, kTubeAcross);
      for (int i = 1; i <= kTubeAlong; ++i) {
        const double f = static_cast<double>(i - 1) / (kTubeAlong - 1);
        const double l = outer + (crease.lv - outer) * f;
        for (int j = 1; j <= kTubeAcross; ++j) {
          const double angle = crease.top + (kPi - crease.top) * (j - 1) / (kTubeAcross - 1);
          above.SetValue(i, j, rim(l, angle, half));
        }
      }
      add(through_grid(above));
      // Below it: from there up to the crease, at each of its samples.
      NCollection_Array2<gp_Pnt> below(1, kTubeAlong, 1, nq);
      for (int k = 1; k <= nq; ++k) {
        const std::size_t c = static_cast<std::size_t>(k) - 1;
        const double angle = crease.angle(c);
        const double end = before ? crease.l1[c] : crease.l2[c];
        for (int i = 1; i <= kTubeAlong; ++i) {
          const double f = static_cast<double>(i - 1) / (kTubeAlong - 1);
          below.SetValue(i, k, rim(outer + (end - outer) * f, angle, half));
        }
      }
      add(through_grid(below));
    }
  }
}

// The solid inside the sewn faces of a tube (and of its bore's tube), its
// open ends (if any) closed by plane faces (rings round a bore): sides
// named by `side_name`, the end whose plane holds `start` named `first`, the
// other `last`. Throws where no sound solid comes of it.
ShapePtr closed_tube(BRepBuilderAPI_Sewing& sewing, double tolerance, const gp_Pnt& start,
                     const std::function<std::string(const TopoDS_Face&)>& side_name, const NameList& first,
                     const NameList& last, const char* what) {
  sewing.Perform();
  const TopoDS_Shape tube = sewing.SewedShape();
  ShapeAnalysis_FreeBounds bounds(tube, tolerance, false, false);
  // The open rims by the plane they lie in, the widest of each the outer.
  struct End {
    gp_Pln plane;
    std::vector<std::pair<double, TopoDS_Wire>> rims;
  };
  std::vector<End> ends;
  for (TopExp_Explorer w(bounds.GetClosedWires(), TopAbs_WIRE); w.More(); w.Next()) {
    const TopoDS_Wire wire = TopoDS::Wire(w.Current());
    BRepBuilderAPI_MakeFace flat(wire, true);
    if (!flat.IsDone()) {
      throw std::runtime_error(std::string(what) + ": an end of the tube could not be closed");
    }
    const gp_Pln plane = BRepAdaptor_Surface(flat.Face()).Plane();
    Bnd_Box box;
    BRepBndLib::Add(wire, box);
    const double size = std::sqrt(box.SquareExtent());
    End* found = nullptr;
    for (End& end : ends) {
      if (end.plane.Distance(plane.Location()) <= 10.0 * tolerance &&
          std::abs(end.plane.Axis().Direction().Dot(plane.Axis().Direction())) > 1.0 - 1.0e-9) {
        found = &end;
      }
    }
    if (found == nullptr) {
      ends.push_back({plane, {}});
      found = &ends.back();
    }
    found->rims.emplace_back(size, wire);
  }
  BRepBuilderAPI_Sewing closing(tolerance);
  closing.Add(tube);
  for (End& end : ends) {
    std::sort(end.rims.begin(), end.rims.end(), [](const auto& a, const auto& b) { return a.first > b.first; });
    BRepBuilderAPI_MakeFace cap(end.rims.front().second, true);
    for (std::size_t k = 1; k < end.rims.size(); ++k) {
      cap.Add(end.rims[k].second);
    }
    if (!cap.IsDone()) {
      throw std::runtime_error(std::string(what) + ": an end of the tube could not be closed");
    }
    ShapeFix_Face fix(cap.Face());
    fix.FixOrientation();
    closing.Add(fix.Face());
  }
  closing.Perform();
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer e(closing.SewedShape(), TopAbs_SHELL); e.More(); e.Next()) {
    shell = TopoDS::Shell(e.Current());
    ++shells;
  }
  if (shells != 1 || closing.NbFreeEdges() != 0) {
    throw std::runtime_error(std::string(what) + ": the tube does not close");
  }
  BRepBuilderAPI_MakeSolid solid(shell);
  const TopoDS_Shape oriented = oriented_solid(solid.Solid(), what);
  FaceNamer namer(oriented);
  for (TopExp_Explorer f(oriented, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() == GeomAbs_Plane) {
      for (const std::string& cap : surface.Plane().Distance(start) <= 10.0 * tolerance ? first : last) {
        namer.add(face, cap);
      }
    } else if (const std::string name = side_name(face); !name.empty()) {
      namer.add(face, name);
    }
  }
  ShapePtr shape = namer.shape();
  if (!is_valid(shape->occt())) {
    throw std::runtime_error(std::string(what) + ": the tube is not a sound solid");
  }
  return shape;
}

// (mitcad#1709) The tube of a circle about a path in no plane whose
// curves join smoothly (a tight bend of a helix inside a longer path):
// where the circle reaches past the centre of a bend the tube meets
// itself, and the points of it nearer another point of the path than its
// radius lie inside the solid. They form a slot round the inside of the
// bend, from a little before it to a little after; the solid's sides are
// the tube less the slot. Positions along the path are X, the index of an
// edge plus the fraction of its curve's parameter.
class PathTube {
public:
  explicit PathTube(const Spine& spine) {
    const int n = spine.edge_count();
    m_edges.resize(static_cast<std::size_t>(n));
    for (int i = 0; i < n; ++i) {
      m_starts.push_back(spine.edge_start(i));
      m_lengths.push_back(spine.edge_length(i));
    }
    int first_curved = -1;
    for (int i = 0; i < n; ++i) {
      Edge& edge = m_edges[static_cast<std::size_t>(i)];
      edge.curve = spine.edge_curve(i);
      edge.straight = spine.edge_straight(i);
      if (!edge.straight && first_curved < 0) {
        first_curved = i;
      }
      for (int k = 0; k <= kSamples; ++k) {
        edge.samples.push_back(point(i, static_cast<double>(k) / kSamples));
      }
    }
    if (first_curved < 0) {
      throw std::invalid_argument("the path has no bend");
    }
    // Frames continuous along the path: a line keeps its neighbour's, a
    // curve its Frenet frame turned to meet its neighbour's.
    for (int i = first_curved + 1; i < n; ++i) {
      join(i, 0.0, frame(i - 1, 1.0).first);
    }
    for (int i = first_curved - 1; i >= 0; --i) {
      join(i, 1.0, frame(i + 1, 0.0).first);
    }
  }

  // The path's point and unit tangent at X.
  void at(double x, gp_Pnt& p, gp_Dir& t) const {
    const auto [i, u] = split(x);
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    t = gp_Dir(d1);
  }

  // The rim's point at X, at the angle from the frame's normal.
  gp_Pnt rim(double x, double angle, double radius) const {
    const auto [i, u] = split(x);
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    const auto [nn, bb] = frame(i, u);
    return p.Translated(gp_Vec(nn) * (radius * std::cos(angle)) + gp_Vec(bb) * (radius * std::sin(angle)));
  }

  // The distance from the point to the path.
  double distance(const gp_Pnt& q) const {
    double best = std::numeric_limits<double>::infinity();
    for (int i = 0; i < static_cast<int>(m_edges.size()); ++i) {
      const Edge& edge = m_edges[static_cast<std::size_t>(i)];
      if (edge.straight) {
        const gp_Pnt a = edge.samples.front();
        const gp_Pnt b = edge.samples.back();
        const gp_Vec d(a, b);
        const double f = std::clamp(gp_Vec(a, q).Dot(d) / d.SquareMagnitude(), 0.0, 1.0);
        best = std::min(best, q.Distance(a.Translated(d * f)));
        continue;
      }
      // About each sample nearer than its neighbours.
      const auto away = [&](double u) {
        gp_Pnt p;
        gp_Vec d1;
        gp_Vec d2;
        derivatives(i, u, p, d1, d2);
        return q.Distance(p);
      };
      for (int k = 0; k <= kSamples; ++k) {
        const double here = q.Distance(edge.samples[static_cast<std::size_t>(k)]);
        best = std::min(best, here);
        if ((k > 0 && q.Distance(edge.samples[static_cast<std::size_t>(k) - 1]) < here) ||
            (k < kSamples && q.Distance(edge.samples[static_cast<std::size_t>(k) + 1]) < here)) {
          continue;
        }
        double lo = static_cast<double>(std::max(0, k - 1)) / kSamples;
        double hi = static_cast<double>(std::min(kSamples, k + 1)) / kSamples;
        for (int it = 0; it < 80; ++it) {
          const double m1 = lo + (hi - lo) / 3.0;
          const double m2 = hi - (hi - lo) / 3.0;
          if (away(m1) < away(m2)) {
            hi = m2;
          } else {
            lo = m1;
          }
        }
        best = std::min(best, away((lo + hi) / 2.0));
      }
    }
    return best;
  }

  int edges() const { return static_cast<int>(m_edges.size()); }

  // How fast the rim's point at X and the angle moves along the path per
  // length of it, square to the tube's sides: 1 - (its offset from the
  // path) . (the path's curvature vector). The tube folds where it is
  // negative (past the centre of the bend), its cuspidal edge where zero.
  // At a joint, of the edge before it when `before`.
  double fold(double x, double angle, double radius, bool before) const {
    auto [i, u] = split(x);
    if (before && i > 0 && u <= 0.0) {
      i -= 1;
      u = 1.0;
    }
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    const auto [nn, bb] = frame(i, u);
    const gp_Vec t = d1.Normalized();
    const gp_Vec k = (d2 - t * t.Dot(d2)) / d1.SquareMagnitude();
    return 1.0 - (gp_Vec(nn) * std::cos(angle) + gp_Vec(bb) * std::sin(angle)).Dot(k) * radius;
  }

  // The angle of the point from the frame's normal at X, about the path.
  double angle(double x, const gp_Pnt& q) const {
    const auto [i, u] = split(x);
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    const auto [nn, bb] = frame(i, u);
    const gp_Vec d(p, q);
    return std::atan2(d.Dot(gp_Vec(bb)), d.Dot(gp_Vec(nn)));
  }

  // Between X and a length along the path (each edge's in proportion to
  // its curve's parameter).
  double to_x(double length) const {
    const int n = edges();
    for (int i = 0; i < n; ++i) {
      const std::size_t k = static_cast<std::size_t>(i);
      if (length < m_starts[k] + m_lengths[k] || i + 1 == n) {
        return i + std::clamp((length - m_starts[k]) / m_lengths[k], 0.0, 1.0);
      }
    }
    return n;
  }
  double to_length(double x) const {
    const auto [i, u] = split(x);
    const std::size_t k = static_cast<std::size_t>(i);
    return m_starts[k] + u * m_lengths[k];
  }
  double total() const { return m_starts.back() + m_lengths.back(); }

  // The local nearests of the path to the point: their distances and
  // lengths along the path.
  std::vector<std::pair<double, double>> nearests(const gp_Pnt& q) const {
    std::vector<std::pair<double, double>> out;
    for (int i = 0; i < static_cast<int>(m_edges.size()); ++i) {
      const Edge& edge = m_edges[static_cast<std::size_t>(i)];
      const std::size_t k0 = static_cast<std::size_t>(i);
      if (edge.straight) {
        const gp_Pnt a = edge.samples.front();
        const gp_Pnt b = edge.samples.back();
        const gp_Vec d(a, b);
        const double f = std::clamp(gp_Vec(a, q).Dot(d) / d.SquareMagnitude(), 0.0, 1.0);
        out.emplace_back(q.Distance(a.Translated(d * f)), m_starts[k0] + f * m_lengths[k0]);
        continue;
      }
      const auto away = [&](double u) {
        gp_Pnt p;
        gp_Vec d1;
        gp_Vec d2;
        derivatives(i, u, p, d1, d2);
        return q.Distance(p);
      };
      for (int k = 0; k <= kSamples; ++k) {
        const double here = q.Distance(edge.samples[static_cast<std::size_t>(k)]);
        if ((k > 0 && q.Distance(edge.samples[static_cast<std::size_t>(k) - 1]) < here) ||
            (k < kSamples && q.Distance(edge.samples[static_cast<std::size_t>(k) + 1]) < here)) {
          continue;
        }
        double lo = static_cast<double>(std::max(0, k - 1)) / kSamples;
        double hi = static_cast<double>(std::min(kSamples, k + 1)) / kSamples;
        for (int it = 0; it < 80; ++it) {
          const double m1 = lo + (hi - lo) / 3.0;
          const double m2 = hi - (hi - lo) / 3.0;
          if (away(m1) < away(m2)) {
            hi = m2;
          } else {
            lo = m1;
          }
        }
        const double u = (lo + hi) / 2.0;
        out.emplace_back(away(u), m_starts[k0] + u * m_lengths[k0]);
      }
    }
    return out;
  }

private:
  struct Edge {
    Spine::EdgeCurve curve;
    bool straight = false;
    double turn = 0.0; // the Frenet frame turned about the tangent
    gp_Dir normal;     // a line's frame
    gp_Dir binormal;
    std::vector<gp_Pnt> samples;
  };

  static constexpr int kSamples = 128;

  std::pair<int, double> split(double x) const {
    const int n = static_cast<int>(m_edges.size());
    const int i = std::clamp(static_cast<int>(std::floor(x)), 0, n - 1);
    return {i, std::clamp(x - i, 0.0, 1.0)};
  }

  void derivatives(int i, double u, gp_Pnt& p, gp_Vec& d1, gp_Vec& d2) const {
    const Spine::EdgeCurve& c = m_edges[static_cast<std::size_t>(i)].curve;
    const double t = c.forward ? c.first + u * (c.last - c.first) : c.last - u * (c.last - c.first);
    c.curve->D2(t, p, d1, d2);
    if (!c.forward) {
      d1.Reverse();
    }
  }

  gp_Pnt point(int i, double u) const {
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    return p;
  }

  std::pair<gp_Dir, gp_Dir> frenet(int i, double u) const {
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    const gp_Dir t(d1);
    const gp_Vec across = d2 - gp_Vec(t) * d2.Dot(gp_Vec(t));
    if (across.Magnitude() < 1.0e-12) {
      throw std::invalid_argument("a curve of the path runs straight at a point");
    }
    const gp_Dir n(across);
    return {n, t.Crossed(n)};
  }

  std::pair<gp_Dir, gp_Dir> frame(int i, double u) const {
    const Edge& edge = m_edges[static_cast<std::size_t>(i)];
    if (edge.straight) {
      return {edge.normal, edge.binormal};
    }
    const auto [n, b] = frenet(i, u);
    const gp_Dir turned(gp_Vec(n) * std::cos(edge.turn) + gp_Vec(b) * std::sin(edge.turn));
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    return {turned, gp_Dir(d1).Crossed(turned)};
  }

  // The edge's frame made to hold `normal` at u.
  void join(int i, double u, const gp_Dir& normal) {
    Edge& edge = m_edges[static_cast<std::size_t>(i)];
    gp_Pnt p;
    gp_Vec d1;
    gp_Vec d2;
    derivatives(i, u, p, d1, d2);
    const gp_Dir t(d1);
    // Square to the tangent (the path joins smoothly, so very nearly so).
    const gp_Dir n(gp_Vec(normal) - gp_Vec(t) * gp_Vec(normal).Dot(gp_Vec(t)));
    if (edge.straight) {
      edge.normal = n;
      edge.binormal = t.Crossed(n);
      return;
    }
    const auto [fn, fb] = frenet(i, u);
    edge.turn = std::atan2(gp_Vec(n).Dot(gp_Vec(fb)), gp_Vec(n).Dot(gp_Vec(fn)));
  }

  std::vector<Edge> m_edges;
  std::vector<double> m_starts;
  std::vector<double> m_lengths;
};

// Adds to `sewing` the two halves of the tube of radius `radius` about the
// path from X `from` to `to` (within one edge), split at the angle `split`
// and opposite it.
void add_ring_faces(BRepBuilderAPI_Sewing& sewing, const PathTube& tube, double radius, double from, double to,
                    double split, double tolerance, const char* what) {
  for (const double start : {split, split + kPi}) {
    NCollection_Array2<gp_Pnt> grid(1, kTubeAlong, 1, kTubeAcross);
    for (int i = 1; i <= kTubeAlong; ++i) {
      const double x = from + (to - from) * (i - 1) / (kTubeAlong - 1);
      for (int j = 1; j <= kTubeAcross; ++j) {
        grid.SetValue(i, j, tube.rim(x, start + kPi * (j - 1) / (kTubeAcross - 1), radius));
      }
    }
    BRepBuilderAPI_MakeFace face(through_grid(grid), tolerance);
    if (!face.IsDone()) {
      throw std::runtime_error(std::string(what) + ": the tube could not be built");
    }
    sewing.Add(face.Face());
  }
}

// Stations along the path where the slot is looked for, per edge, and
// angles round the rim.
constexpr int kSlotStations = 64;
constexpr int kSlotAngles = 96;

// Adds to `sewing` the faces of the tube of radius `radius` about the path
// less its slot (one slot, of one run of angles at each station; throws
// otherwise), split at the path's joints. The slot's edge is the crease
// where the tube meets itself, traced from a point of it both ways to its
// corners, where the crease meets the tube's cuspidal edge (the other
// point of the path that reaches it comes to the station itself), and the
// same crease seen from the other point back again. Its corners are
// solved exactly where they lie at a joint of the path (where the bend
// jumps, as at the ends of a helix between straight legs); a corner inside
// a curve is not built here.
void add_slotted_faces(BRepBuilderAPI_Sewing& sewing, const PathTube& tube, double radius, double tolerance,
                       const char* what) {
  const double slack = 1.0e-9 * std::max(1.0, radius);
  // How far inside the solid the rim's point is (positive in the slot).
  const auto inside = [&](double x, double angle) { return radius - tube.distance(tube.rim(x, angle, radius)); };
  const int n = tube.edges();
  // Scan: the stations with points in the slot, and a middle angle there.
  std::vector<double> xs;
  for (int i = 0; i < n; ++i) {
    for (int k = 0; k < kSlotStations; ++k) {
      xs.push_back(i + static_cast<double>(k) / kSlotStations);
    }
  }
  xs.push_back(n);
  int first = -1;
  int last = -1;
  std::vector<double> middles(xs.size(), 0.0);
  std::vector<int> widths(xs.size(), 0);
  for (std::size_t k = 0; k < xs.size(); ++k) {
    std::vector<bool> in(kSlotAngles);
    int count = 0;
    for (int j = 0; j < kSlotAngles; ++j) {
      in[static_cast<std::size_t>(j)] = inside(xs[k], kTwoPi * j / kSlotAngles) > slack;
      count += in[static_cast<std::size_t>(j)] ? 1 : 0;
    }
    if (count == 0) {
      continue;
    }
    // One run of angles, round the circle.
    int starts = 0;
    int start = 0;
    for (int j = 0; j < kSlotAngles; ++j) {
      if (in[static_cast<std::size_t>(j)] && !in[static_cast<std::size_t>((j + kSlotAngles - 1) % kSlotAngles)]) {
        ++starts;
        start = j;
      }
    }
    if (starts != 1 || count == kSlotAngles) {
      throw std::runtime_error(std::string(what) + ": the tube meets itself in a way not built here");
    }
    middles[k] = kTwoPi * (start + (count - 1) / 2.0) / kSlotAngles;
    widths[k] = count;
    if (first < 0) {
      first = static_cast<int>(k);
    } else if (last != static_cast<int>(k) - 1) {
      throw std::runtime_error(std::string(what) + ": the tube meets itself in more than one place");
    }
    last = static_cast<int>(k);
  }
  if (first <= 0 || last < 0 || last + 1 >= static_cast<int>(xs.size())) {
    throw std::runtime_error(std::string(what) + ": the tube meets itself where it is not built here");
  }
  // The slot's edge: where the rim's point lies as far from the path at
  // another point y (in y's plane square to the path) as from its own.
  // Lengths along the path: s the station's, y the other's; a the angle.
  const auto crease = [&](double sl, double a, double y) {
    const gp_Pnt q = tube.rim(tube.to_x(sl), a, radius);
    gp_Pnt c;
    gp_Dir t;
    tube.at(tube.to_x(y), c, t);
    const gp_Vec d(c, q);
    return std::make_pair(d.Dot(gp_Vec(t)), (d.SquareMagnitude() - radius * radius) / (2.0 * radius));
  };
  const double step = 1.0e-7 * std::max(1.0, radius);
  // At a station, from a guess: the angle and the other point.
  const auto solve = [&](double sl, double& a, double& y) {
    for (int it = 0; it < 40; ++it) {
      const auto [f1, f2] = crease(sl, a, y);
      if (std::hypot(f1, f2) < 1.0e-12 * std::max(1.0, radius)) {
        return std::abs(y - sl) > 1.0e-3 * radius;
      }
      const double da = step / radius;
      const auto [a1, a2] = crease(sl, a + da, y);
      const auto [y1, y2] = crease(sl, a, y + step);
      const double j11 = (a1 - f1) / da;
      const double j21 = (a2 - f2) / da;
      const double j12 = (y1 - f1) / step;
      const double j22 = (y2 - f2) / step;
      const double det = j11 * j22 - j12 * j21;
      if (std::abs(det) < 1.0e-300) {
        return false;
      }
      a -= (f1 * j22 - f2 * j12) / det;
      y -= (j11 * f2 - j21 * f1) / det;
    }
    return false;
  };
  // A start on the slot's edge at its widest station.
  // (Away from the path's joints, where the other point may lie close.)
  std::size_t widest = static_cast<std::size_t>(first);
  int widest_width = -1;
  for (std::size_t k = static_cast<std::size_t>(first); k <= static_cast<std::size_t>(last); ++k) {
    const double off = xs[k] - std::round(xs[k]);
    if (std::abs(off) > 0.2 && widths[k] > widest_width) {
      widest = k;
      widest_width = widths[k];
    }
  }
  const double s0 = tube.to_length(xs[widest]);
  double a0 = 0.0;
  {
    double in = middles[widest];
    double out = in + kPi;
    for (int it = 0; it < 60; ++it) {
      const double m = (in + out) / 2.0;
      (inside(xs[widest], m) > 0.0 ? in : out) = m;
    }
    a0 = (in + out) / 2.0;
  }
  double y0 = 0.0;
  {
    double best = std::numeric_limits<double>::infinity();
    for (const auto& [d, y] : tube.nearests(tube.rim(xs[widest], a0, radius))) {
      if (std::abs(y - s0) > 0.05 * radius && d < best) {
        best = d;
        y0 = y;
      }
    }
    if (!(best < radius * 1.001) || !solve(s0, a0, y0)) {
      throw std::runtime_error(std::string(what) + ": the start of the tube's slot is not found");
    }
  }
  // Along the slot's edge (s, a, y), by steps along it: a step along the
  // curve's tangent, then back onto it square to that. Steps shrink where
  // the other point nears the station: a corner of the slot, where its
  // edge meets the tube's fold and turns to the same crease seen from the
  // other point (as at the tightest point of a curve in one plane).
  struct Point {
    double s;
    double a;
    double y;
  };
  const auto gradient = [&](const Point& at) {
    const auto [f1, f2] = crease(at.s, at.a, at.y);
    const auto [s1, s2] = crease(at.s + step, at.a, at.y);
    const auto [a1, a2] = crease(at.s, at.a + step / radius, at.y);
    const auto [y1, y2] = crease(at.s, at.a, at.y + step);
    return std::make_pair(gp_Vec((s1 - f1) / step, (a1 - f1) / step, (y1 - f1) / step),
                          gp_Vec((s2 - f2) / step, (a2 - f2) / step, (y2 - f2) / step));
  };
  const double h = 0.01 * radius;
  const double near_corner = 2.0e-3 * radius;
  // Follows the edge from the start one way: to a corner (true) or round
  // back to the start (false).
  const auto follow = [&](double way, std::vector<Point>& run) {
    run.assign(1, Point{s0, a0, y0});
    gp_Vec previous;
    for (int k = 0; k < 20000; ++k) {
      const Point& p = run.back();
      const auto [g1, g2] = gradient(p);
      gp_Vec tangent = g1.Crossed(g2);
      if (tangent.Magnitude() < 1.0e-12) {
        throw std::runtime_error(std::string(what) + ": the edge of the tube's slot is not followed");
      }
      tangent.Normalize();
      if (k == 0 ? way < 0.0 : tangent.Dot(previous) < 0.0) {
        tangent.Reverse();
      }
      previous = tangent;
      const double length = std::min(h, 0.25 * std::abs(p.y - p.s));
      Point next{p.s + length * tangent.X(), p.a + length * tangent.Y() / radius, p.y + length * tangent.Z()};
      const Point guess = next;
      bool converged = false;
      for (int it = 0; it < 40 && !converged; ++it) {
        const auto [f1, f2] = crease(next.s, next.a, next.y);
        const double f3 = (next.s - guess.s) * tangent.X() + (next.a - guess.a) * radius * tangent.Y() +
                          (next.y - guess.y) * tangent.Z();
        if (std::abs(f1) + std::abs(f2) + std::abs(f3) < 1.0e-12 * std::max(1.0, radius)) {
          converged = true;
          break;
        }
        const auto [r1, r2] = gradient(next);
        const gp_Mat m(r1.X(), r1.Y(), r1.Z(), r2.X(), r2.Y(), r2.Z(), tangent.X(), tangent.Y(), tangent.Z());
        if (std::abs(m.Determinant()) < 1.0e-300) {
          break;
        }
        const gp_XYZ d = m.Inverted() * gp_XYZ(-f1, -f2, -f3);
        next.s += d.X();
        next.a += d.Y() / radius;
        next.y += d.Z();
      }
      if (!converged) {
        throw std::runtime_error(std::string(what) + ": the edge of the tube's slot is not followed");
      }
      run.push_back(next);
      if (std::abs(next.y - next.s) < near_corner) {
        return true;
      }
      if (run.size() > 20 && std::hypot(next.s - s0, (next.a - a0) * radius, next.y - y0) < 1.5 * h) {
        run.pop_back();
        return false;
      }
    }
    throw std::runtime_error(std::string(what) + ": the edge of the tube's slot does not end");
  };
  // A corner: each of s, a, y on the last points, a parabola in y - s,
  // taken at y = s.
  const auto corner_of = [&](const std::vector<Point>& run) {
    const Point& p0 = run[run.size() - 3];
    const Point& p1 = run[run.size() - 2];
    const Point& p2 = run[run.size() - 1];
    const double d0 = p0.y - p0.s;
    const double d1 = p1.y - p1.s;
    const double d2 = p2.y - p2.s;
    const auto at_zero = [&](double v0, double v1, double v2) {
      // Lagrange's parabola through (d, v) at d = 0.
      return v0 * (d1 * d2) / ((d0 - d1) * (d0 - d2)) + v1 * (d0 * d2) / ((d1 - d0) * (d1 - d2)) +
             v2 * (d0 * d1) / ((d2 - d0) * (d2 - d1));
    };
    const double sc = at_zero(p0.s, p1.s, p2.s);
    return Point{sc, at_zero(p0.a, p1.a, p2.a), sc};
  };
  // A corner exactly: where the crease meets the tube's cuspidal edge
  // (fold zero). At a joint of the path where the curvature jumps, the
  // cuspidal edge starts there, on the side that folds. (Inside a curve
  // a corner lies where the cuspidal edge runs along the direction the
  // tube folds in; not built here.)
  const auto exact_corner = [&](const std::vector<Point>& run) {
    const Point guess = corner_of(run);
    for (int i = 1; i < n; ++i) {
      const double joint = tube.to_length(i);
      if (std::abs(guess.s - joint) > 1.0e-4 * std::max(1.0, radius)) {
        continue;
      }
      std::optional<double> best;
      for (const bool before : {true, false}) {
        double a = guess.a;
        bool converged = false;
        for (int it = 0; it < 50 && !converged; ++it) {
          const double f = tube.fold(i, a, radius, before);
          const double da = 1.0e-6;
          const double df = (tube.fold(i, a + da, radius, before) - tube.fold(i, a - da, radius, before)) / (2.0 * da);
          if (std::abs(df) < 1.0e-9) {
            break;
          }
          a -= f / df;
          converged = std::abs(f / df) < 1.0e-14;
        }
        if (converged && std::abs(a - guess.a) < 0.2 && (!best || std::abs(a - guess.a) < std::abs(*best - guess.a))) {
          best = a;
        }
      }
      if (!best) {
        break;
      }
      return Point{joint, *best, joint};
    }
    throw std::runtime_error(std::string(what) + ": the tube meets itself where it is not built here");
  };
  std::vector<Point> forward;
  std::vector<Point> backward;
  std::vector<Point> loop;
  std::vector<std::size_t> corners;
  if (!follow(1.0, forward)) {
    loop = forward;
  } else {
    if (!follow(-1.0, backward)) {
      throw std::runtime_error(std::string(what) + ": the edge of the tube's slot is not followed");
    }
    // The crease from one corner to the other, then the same seen from
    // the other point back again.
    for (std::vector<Point>* run : {&forward, &backward}) {
      const Point corner = exact_corner(*run);
      const Point& end = (*run)[run->size() - 1];
      const Point& before = (*run)[run->size() - 2];
      if ((corner.s - end.s) * (end.s - before.s) > 0.0) {
        run->push_back(corner);
      } else {
        run->back() = corner;
      }
    }
    std::vector<Point> arc(backward.rbegin(), backward.rend());
    arc.insert(arc.end(), forward.begin() + 1, forward.end());
    corners.push_back(0);
    loop = arc;
    corners.push_back(loop.size() - 1);
    for (std::size_t k = arc.size() - 2; k >= 1; --k) {
      const Point& p = arc[k];
      const double x = tube.to_x(p.y);
      double a = tube.angle(x, tube.rim(tube.to_x(p.s), p.a, radius));
      a += kTwoPi * std::round((loop.back().a - a) / kTwoPi);
      loop.push_back({p.y, a, p.s});
    }
  }
  const std::size_t count = loop.size();
  const auto is_corner = [&](std::size_t k) { return std::find(corners.begin(), corners.end(), k) != corners.end(); };
  // Its ends: where it turns back along the path (a parabola through the
  // three points about the farthest).
  const auto turning = [&](std::size_t k) {
    const Point& b = loop[(k + count - 1) % count];
    const Point& m = loop[k];
    const Point& f = loop[(k + 1) % count];
    const double curve = b.s - 2.0 * m.s + f.s;
    const double t = std::abs(curve) > 1.0e-300 ? -(f.s - b.s) / (2.0 * curve) : 0.0;
    const auto at = [&](double vb, double vm, double vf) {
      return vm + (vf - vb) / 2.0 * t + (vb - 2.0 * vm + vf) / 2.0 * t * t;
    };
    return Point{at(b.s, m.s, f.s), at(b.a, m.a, f.a), at(b.y, m.y, f.y)};
  };
  std::size_t ka = 0;
  std::size_t kb = 0;
  for (std::size_t k = 0; k < count; ++k) {
    ka = loop[k].s < loop[ka].s ? k : ka;
    kb = loop[k].s > loop[kb].s ? k : kb;
  }
  if (is_corner(ka) || is_corner(kb)) {
    throw std::runtime_error(std::string(what) + ": the tube's slot ends at a corner");
  }
  const Point start = turning(ka);
  const Point finish = turning(kb);
  const double xa = tube.to_x(start.s);
  const double xb = tube.to_x(finish.s);
  const double ea = start.a;
  double eb = finish.a;
  eb += kTwoPi * std::round((ea - eb) / kTwoPi);
  // The two runs from end to end, each rising along the path; the one of
  // larger angles the slot's upper edge. (Corners marked.)
  struct Run {
    std::vector<Point> points;
    std::vector<bool> corner;
  };
  Run one;
  Run other;
  for (std::size_t k = ka;; k = (k + 1) % count) {
    one.points.push_back(loop[k]);
    one.corner.push_back(is_corner(k));
    if (k == kb) {
      break;
    }
  }
  for (std::size_t k = ka;; k = (k + count - 1) % count) {
    other.points.push_back(loop[k]);
    other.corner.push_back(is_corner(k));
    if (k == kb) {
      break;
    }
  }
  std::vector<double> corner_stations;
  for (Run* run : {&one, &other}) {
    std::vector<Point>& points = run->points;
    for (std::size_t k = 1; k < points.size(); ++k) {
      Point& p = points[k];
      p.a += kTwoPi * std::round((points[k - 1].a - p.a) / kTwoPi);
      if (!(p.s > points[k - 1].s)) {
        throw std::runtime_error(std::string(what) + ": the tube's slot is not built here");
      }
      if (run->corner[k]) {
        corner_stations.push_back(p.s);
      }
    }
  }
  // Where the other point crosses a joint of the path the slot's edge
  // kinks too: the station there (the crease solved with the other point
  // at the joint).
  for (const Run* run : {&one, &other}) {
    const std::vector<Point>& points = run->points;
    for (int i = 1; i < n; ++i) {
      const double joint = tube.to_length(i);
      for (std::size_t k = 1; k < points.size(); ++k) {
        const Point& b = points[k - 1];
        const Point& f = points[k];
        // (Not at a corner on the joint, where the other point is the station.)
        if ((b.y - joint) * (f.y - joint) >= 0.0 || std::abs(b.y - b.s) < 0.01 * radius) {
          continue;
        }
        const double t = (joint - b.y) / (f.y - b.y);
        double sl = b.s + (f.s - b.s) * t;
        double a = b.a + (f.a - b.a) * t;
        for (int it = 0; it < 40; ++it) {
          const auto [f1, f2] = crease(sl, a, joint);
          if (std::hypot(f1, f2) < 1.0e-12 * std::max(1.0, radius)) {
            break;
          }
          const double da = step / radius;
          const auto [s1, s2] = crease(sl + step, a, joint);
          const auto [a1, a2] = crease(sl, a + da, joint);
          const double j11 = (s1 - f1) / step;
          const double j21 = (s2 - f2) / step;
          const double j12 = (a1 - f1) / da;
          const double j22 = (a2 - f2) / da;
          const double det = j11 * j22 - j12 * j21;
          if (std::abs(det) < 1.0e-300) {
            break;
          }
          sl -= (f1 * j22 - f2 * j12) / det;
          a -= (j11 * f2 - j21 * f1) / det;
        }
        corner_stations.push_back(sl);
      }
    }
  }
  const bool one_upper = one.points[one.points.size() / 2].a > other.points[other.points.size() / 2].a;
  const Run& upper_run = one_upper ? one : other;
  const Run& lower_run = one_upper ? other : one;
  // The slot's edge on a run at a station inside the slot.
  const auto edge_on = [&](const Run& on, double sl) {
    const std::vector<Point>& run = on.points;
    for (std::size_t c = 0; c < run.size(); ++c) {
      if (on.corner[c] && std::abs(run[c].s - sl) <= 1.0e-6 * std::max(1.0, radius)) {
        return run[c].a;
      }
    }
    std::size_t k = 1;
    while (k + 1 < run.size() && run[k].s < sl) {
      ++k;
    }
    const Point& b = run[k - 1];
    const Point& f = run[k];
    const double t = std::clamp((sl - b.s) / (f.s - b.s), 0.0, 1.0);
    double a = b.a + (f.a - b.a) * t;
    double y = b.y + (f.y - b.y) * t;
    const double guess = a;
    if (solve(sl, a, y) && std::abs(a - guess) < 0.5) {
      return a;
    }
    throw std::runtime_error(std::string(what) + ": the edge of the tube's slot is not found");
  };
  // The angle between the halves of the tube: opposite the slot.
  const auto opposite = [&](double x) { return kPi + ea + (eb - ea) * (x - xa) / (xb - xa); };
  const auto add = [&](const NCollection_Array2<gp_Pnt>& grid) {
    BRepBuilderAPI_MakeFace face(through_grid(grid), tolerance);
    if (!face.IsDone()) {
      throw std::runtime_error(std::string(what) + ": the tube could not be built");
    }
    sewing.Add(face.Face());
  };
  // Outside the slot: the two halves of the tube, split where the slot's
  // nearest end lies and opposite it, each edge on its own.
  for (int i = 0; i < n; ++i) {
    if (i < xa) {
      add_ring_faces(sewing, tube, radius, i, std::min<double>(i + 1, xa), ea, tolerance, what);
    }
    if (i + 1 > xb) {
      add_ring_faces(sewing, tube, radius, std::max<double>(i, xb), i + 1, eb, tolerance, what);
    }
  }
  // Along the slot: from its edges round to the angle opposite it, each
  // stretch between the slot's ends and the joints on its own, quadratic
  // in its parameter at the slot's ends (where the slot's edges meet).
  std::vector<double> bounds{xa};
  for (int i = 1; i < n; ++i) {
    if (i > xa && i < xb) {
      bounds.push_back(i);
    }
  }
  for (const double corner : corner_stations) {
    // (A corner at a joint is taken at the joint.)
    if (std::none_of(bounds.begin(), bounds.end(), [&](double b) {
          return std::abs(tube.to_length(b) - corner) <= 1.0e-6 * std::max(1.0, radius);
        })) {
      bounds.push_back(tube.to_x(corner));
    }
  }
  bounds.push_back(xb);
  std::sort(bounds.begin(), bounds.end());
  // Whether a run's edge leaves the station towards `side` (+1 along the
  // path, -1 back) square to the path, as a square root of the length from
  // it: at a corner, on the side where the mirror of the crease runs (on
  // the other the edge leaves at a slope). Its angle against the length
  // from the corner, at two of the run's points, one at least four times
  // as far as the other.
  const auto steep = [&](const Run& on, double station, double side) {
    const std::vector<Point>& run = on.points;
    const double near = 1.0e-6 * std::max(1.0, radius);
    std::optional<std::size_t> corner;
    for (std::size_t c = 0; c < run.size(); ++c) {
      if (on.corner[c] && std::abs(run[c].s - station) <= near) {
        corner = c;
      }
    }
    if (!corner) {
      return false;
    }
    const Point& at = run[*corner];
    std::optional<std::pair<double, double>> nearer;
    for (std::size_t k = 0; k < run.size(); ++k) {
      const double d = (run[k].s - at.s) * side;
      if (!(d > near)) {
        continue;
      }
      const double turn = std::abs(run[k].a - at.a);
      if (!nearer || d < nearer->first) {
        nearer = std::make_pair(d, turn);
      }
    }
    if (!nearer) {
      return false;
    }
    std::optional<std::pair<double, double>> farther;
    for (std::size_t k = 0; k < run.size(); ++k) {
      const double d = (run[k].s - at.s) * side;
      if (d >= 4.0 * nearer->first && (!farther || d < farther->first)) {
        farther = std::make_pair(d, std::abs(run[k].a - at.a));
      }
    }
    if (!farther || !(nearer->second > 0.0) || !(farther->second > 0.0)) {
      return false;
    }
    const double power = std::log(farther->second / nearer->second) / std::log(farther->first / nearer->first);
    return power < 0.75;
  };
  for (std::size_t k = 0; k + 1 < bounds.size(); ++k) {
    const double from = bounds[k];
    const double to = bounds[k + 1];
    const bool opens = k == 0;
    const bool closes = k + 2 == bounds.size();
    for (const bool up : {true, false}) {
      const Run& run = up ? upper_run : lower_run;
      // Rows close up quadratically where the edge leaves square to the
      // path (at the slot's ends and on one side of its corners), so that
      // they follow it; elsewhere evenly, so that no row of the face
      // shrinks to a point.
      const bool grade_from = opens || steep(run, tube.to_length(from), 1.0);
      const bool grade_to = closes || steep(run, tube.to_length(to), -1.0);
      NCollection_Array2<gp_Pnt> grid(1, kTubeAlong, 1, kTubeAcross);
      for (int i = 1; i <= kTubeAlong; ++i) {
        const double f = static_cast<double>(i - 1) / (kTubeAlong - 1);
        const double g = grade_from && grade_to ? (1.0 - std::cos(kPi * f)) / 2.0
                         : grade_from           ? f * f
                         : grade_to             ? 1.0 - (1.0 - f) * (1.0 - f)
                                                : f;
        const double x = from + (to - from) * g;
        double edge = 0.0;
        if ((opens && i == 1) || (closes && i == kTubeAlong)) {
          edge = i == 1 ? ea : eb;
        } else {
          edge = edge_on(run, tube.to_length(x));
        }
        const double split = opposite(x);
        if (up ? !(edge < split) : !(split < edge + kTwoPi)) {
          throw std::runtime_error(std::string(what) + ": the slot in the tube runs round it");
        }
        for (int j = 1; j <= kTubeAcross; ++j) {
          const double part = static_cast<double>(j - 1) / (kTubeAcross - 1);
          grid.SetValue(i, j,
                        up ? tube.rim(x, edge + (split - edge) * part, radius)
                           : tube.rim(x, split + (edge + kTwoPi - split) * part, radius));
        }
      }
      add(grid);
    }
  }
}

// How a bend of the path is built: as the rest of the path (the profile
// does not reach past its centre, or a fold not built in pieces), turned
// about the axis of a circular arc, or (mitcad#1709) from its tube up to
// where it meets itself: in a path in one plane (add_creased_faces) or in
// none (add_slotted_faces), the whole path as its tube (whole_tube).
enum class Bend { None, Turned, Creased, Slotted };

class Unfolder {
public:
  // The frames along the spine: with a constant `binormal`, else (the
  // corrected Frenet frame) carried along the spine from the profile.
  Unfolder(const FoldedSweep& sweep, const std::vector<SweptProfile>& profiles, const std::optional<gp_Dir>& binormal)
      : m_sweep(sweep), m_spine(*sweep.spine), m_profiles(profiles), m_binormal(binormal) {
    m_length = m_spine.length();
    m_eps = 1.0e-9 * std::max(1.0, m_length);
    const Station at = station(sweep.at, true);
    m_round = round_profiles(profiles, m_spine.edge_point(at.edge, at.local), m_spine.edge_tangent(at.edge, at.local),
                             1.0e-7 * std::max(1.0, m_length));
    if (!m_binormal && !m_round) {
      carry_frames(at);
    }
    if (m_round && profiles.size() == 1) {
      for (const NamedWire& loop : profiles.front().loops) {
        TopExp_Explorer e(loop.wire, TopAbs_EDGE);
        m_radii.push_back(BRepAdaptor_Curve(TopoDS::Edge(e.Current())).Circle().Radius());
      }
    }
    m_at = frame(at);
    // A profile at a corner: which side's frame it lies in is not known.
    for (const Joint& joint : joints()) {
      if (std::abs(joint.s - sweep.at) <= 1.0e-6 && joint.turn > 1.0e-9 && joint.s > m_eps &&
          joint.s < m_length - m_eps) {
        throw std::invalid_argument("the profile lies at a corner of the path");
      }
    }
    for (const SweptProfile& profile : profiles) {
      for (const NamedWire& loop : profile.loops) {
        for (TopExp_Explorer e(loop.wire, TopAbs_EDGE); e.More(); e.Next()) {
          const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
          for (int k = 0; k <= kProfileSamples; ++k) {
            const double t = curve.FirstParameter() +
                             (curve.LastParameter() - curve.FirstParameter()) * k / kProfileSamples;
            m_samples.push_back(curve.Value(t));
          }
        }
      }
    }
    const gp_Pnt origin = m_at.Location();
    for (const gp_Pnt& p : m_samples) {
      m_reach = std::max(m_reach, p.Distance(origin));
    }
    m_slack = 1.0e-9 * std::max(1.0, m_reach);
    m_joints = joints();
    find_bends();
    m_cuts = folded_legs({});
    m_folds = m_folds || !m_cuts.empty();
    // A miter where the path turns right back has no room anywhere.
    for (const Joint& joint : m_joints) {
      if (mitred(joint) && joint.turn > kTurnBack) {
        m_folds = true;
        m_unresolvable = true;
        if (!m_turns_back) {
          m_turns_back = true;
          m_back = m_spine.edge_point(joint.after, 0.0);
          note_where(m_back);
        }
      }
    }
    find_return();
    // (mitcad#1709) Bends built from their tube only as the whole tube.
    if ((!m_creases.empty() || slotted()) && !tube_only()) {
      m_unresolvable = true;
    }
  }

  bool unresolvable() const { return m_unresolvable; }
  // Where the path turns right back at a mitered corner, if it does.
  std::optional<gp_Pnt> turning_back() const {
    return m_turns_back ? std::optional<gp_Pnt>(m_back) : std::nullopt;
  }
  bool folds() const { return m_folds; }
  bool returns() const { return m_returning < std::numeric_limits<double>::infinity(); }
  const gp_Pnt& where() const { return m_where; }

  bool has_corner() const {
    return std::any_of(m_joints.begin(), m_joints.end(), [&](const Joint& j) { return sharp(j); });
  }

  // Where the sweep is cut: at both ends of each arc the profile reaches
  // past the centre of and at the corners of each straight piece too short
  // for its miters (`at_corners`: at every corner), and where the path
  // comes back near itself, between the cuts, so that no piece does.
  std::vector<double> plan(bool at_corners) const {
    std::vector<double> cuts = m_cuts;
    for (int i = 0; i < m_spine.edge_count(); ++i) {
      if (bend(i) != Bend::None) {
        add_cut(cuts, m_spine.edge_start(i));
        add_cut(cuts, m_spine.edge_start(i) + m_spine.edge_length(i));
      }
    }
    if (at_corners) {
      for (const Joint& joint : m_joints) {
        if (sharp(joint)) {
          add_cut(cuts, joint.s);
        }
      }
    }
    for (const double s : folded_legs(cuts)) {
      add_cut(cuts, s);
    }
    if (returns()) {
      for (const double s : returning_cuts(cuts)) {
        add_cut(cuts, s);
      }
      for (const double s : folded_legs(cuts)) {
        add_cut(cuts, s);
      }
    }
    return cuts;
  }

  // The pieces between the cuts, and the joints at the corners cut,
  // fused.
  ShapePtr pieces_fused(const std::vector<double>& cuts) const {
    std::vector<double> bounds{0.0};
    for (const double s : cuts) {
      if (s > m_eps && s < m_length - m_eps) {
        bounds.push_back(s);
      }
    }
    bounds.push_back(m_length);
    std::sort(bounds.begin(), bounds.end());
    std::vector<ShapePtr> parts;
    for (const SweptProfile& profile : m_profiles) {
      for (std::size_t k = 0; k + 1 < bounds.size(); ++k) {
        throw_if_cancelled();
        const double from = bounds[k];
        const double to = bounds[k + 1];
        const NameList& first = !m_sweep.around && k == 0 ? profile.first : m_none;
        const NameList& last = !m_sweep.around && k + 2 == bounds.size() ? profile.last : m_none;
        const Station start = station(from, true);
        const std::vector<NamedWire> loops = placed(profile.loops, move_to(start));
        const int edge = start.edge;
        const double edge_end = m_spine.edge_start(edge) + m_spine.edge_length(edge);
        if (bend(edge) == Bend::Turned && std::abs(from - m_spine.edge_start(edge)) <= m_eps &&
            std::abs(to - edge_end) <= m_eps) {
          const gp_Circ circle = *m_spine.edge_circle(edge);
          for (const ShapePtr& part : turned(loops, circle.Axis(), m_spine.edge_length(edge) / circle.Radius(),
                                             first, last, m_sweep.what)) {
            parts.push_back(part);
          }
          continue;
        }
        const TopoDS_Wire wire = m_spine.part(from, to);
        const ShapePtr outer =
            pipe_shell(loops.front(), wire, m_sweep.mode, first, last, m_sweep.tolerance, m_sweep.corners);
        std::vector<ShapePtr> holes;
        for (std::size_t l = 1; l < loops.size(); ++l) {
          holes.push_back(pipe_shell(loops[l], wire, m_sweep.mode, {}, {}, m_sweep.tolerance, m_sweep.corners));
        }
        parts.push_back(with_holes(outer, holes, m_sweep.what));
      }
      for (const Joint& joint : m_joints) {
        if (!sharp(joint) || !(cut_at(cuts, joint.s) || (m_sweep.around && joint.s <= m_eps))) {
          continue;
        }
        throw_if_cancelled();
        for (const ShapePtr& part : joined(profile, joint)) {
          parts.push_back(part);
        }
      }
    }
    ShapePtr result = fused(parts, m_sweep.what, false, m_sweep.tolerance);
    // Sound, and the pieces of a profile one solid.
    std::size_t solids = 0;
    for (TopExp_Explorer s(result->occt(), TopAbs_SOLID); s.More(); s.Next()) {
      ++solids;
    }
    if (!sound(*result) || solids > m_profiles.size()) {
      throw std::runtime_error(std::string(m_sweep.what) + ": its pieces do not make one sound solid");
    }
    // (mitcad#1708) The union holds at least its largest piece and at
    // most all of them: a boolean that went wrong where pieces touch
    // tangentially can still give a valid solid.
    double largest = 0.0;
    double all = 0.0;
    for (const ShapePtr& part : parts) {
      GProp_GProps props;
      BRepGProp::VolumeProperties(part->occt(), props);
      largest = std::max(largest, props.Mass());
      all += props.Mass();
    }
    GProp_GProps props;
    BRepGProp::VolumeProperties(result->occt(), props);
    if (props.Mass() < largest * (1.0 - 1.0e-6) || props.Mass() > all * (1.0 + 1.0e-6)) {
      throw std::runtime_error(std::string(m_sweep.what) + ": its pieces' union does not hold them");
    }
    return result;
  }

  // (mitcad#1709) Whether the sweep is built as its tube (whole_tube): it
  // folds only at bends built from their tube (Bend::Creased in one plane,
  // Bend::Slotted in none), round one profile whose path joins smoothly
  // everywhere and does not come back near itself, a bore below the
  // tightest bend.
  bool tube_only() const {
    if ((m_creases.empty() && !slotted()) || (!m_creases.empty() && slotted()) || m_unresolvable ||
        !m_cuts.empty() || returns() || m_profiles.size() != 1) {
      return false;
    }
    // A bore that does not reach past a bend's centre (no wall fills it).
    if (m_radii.size() > 1 && !(m_radii.back() < m_tightest * (1.0 - 1.0e-9))) {
      return false;
    }
    for (int i = 0; i < m_spine.edge_count(); ++i) {
      if (bend(i) == Bend::Turned) {
        return false;
      }
    }
    return std::none_of(m_joints.begin(), m_joints.end(), [&](const Joint& joint) { return sharp(joint); });
  }

  bool slotted() const { return std::find(m_bend.begin(), m_bend.end(), Bend::Slotted) != m_bend.end(); }

  // (mitcad#1709) The sweep as the tube the profile's rim sweeps: in one
  // plane along each curve of the path its two halves (either side of the
  // path's plane), exact across, and along a bend built from its tube its
  // faces up to the crease (add_creased_faces); in no plane the whole
  // tube less its slot (add_slotted_faces). Sewn and closed at the path's
  // ends; the bore's tube (which does not fold) cut out.
  ShapePtr whole_tube() const {
    const double radius = m_radii.front();
    const double tolerance = 1.0e-7 * std::max(1.0, radius);
    BRepBuilderAPI_Sewing sewing(10.0 * tolerance);
    if (slotted()) {
      // In no plane: the tube less its slot, the bore's tube whole.
      const PathTube tube(m_spine);
      add_slotted_faces(sewing, tube, radius, tolerance, m_sweep.what);
      if (m_radii.size() > 1) {
        for (int i = 0; i < m_spine.edge_count(); ++i) {
          add_ring_faces(sewing, tube, m_radii[1], i, i + 1, 0.0, tolerance, m_sweep.what);
        }
      }
    }
    for (std::size_t r = 0; r < m_radii.size() && !slotted(); ++r) {
      for (int i = 0; i < m_spine.edge_count(); ++i) {
        throw_if_cancelled();
        const PlaneCurve curve(m_spine.edge_curve(i), *m_binormal);
        // The rim's tube meets itself at bends built from it; the bore's
        // never does (its radius is below their bends').
        if (r == 0 && bend(i) == Bend::Creased) {
          add_creased_faces(sewing, curve, m_creases.at(i), m_radii[r], tolerance, m_sweep.what);
          continue;
        }
        for (const double half : {1.0, -1.0}) {
          BRepBuilderAPI_MakeFace face(half_tube(curve, 1.0, m_radii[r], 0.0, curve.span(), half), tolerance);
          if (!face.IsDone()) {
            throw std::runtime_error(std::string(m_sweep.what) + ": the tube could not be built");
          }
          sewing.Add(face.Face());
        }
      }
    }
    const SweptProfile& profile = m_profiles.front();
    const auto name = [&](std::size_t loop) {
      const std::vector<std::pair<TopoDS_Edge, std::string>>& edges = profile.loops[loop].edges;
      return edges.empty() ? std::string() : edges.front().second;
    };
    // A side's name: the rim's, or the bore's where it lies nearer the
    // bore's radius from the path.
    const auto side_name = [&](const TopoDS_Face& face) {
      if (m_radii.size() < 2) {
        return name(0);
      }
      double u0 = 0.0;
      double u1 = 0.0;
      double v0 = 0.0;
      double v1 = 0.0;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      const gp_Pnt p = BRep_Tool::Surface(face)->Value((u0 + u1) / 2.0, (v0 + v1) / 2.0);
      const double away = p.Distance(m_spine.point(m_spine.nearest(p)));
      return std::abs(away - m_radii[1]) < std::abs(away - m_radii[0]) ? name(1) : name(0);
    };
    return closed_tube(sewing, 10.0 * tolerance, m_spine.edge_point(0, 0.0), side_name,
                       m_sweep.around ? m_none : profile.first, m_sweep.around ? m_none : profile.last, m_sweep.what);
  }

private:
  // The station at arc length `s`: on the edge that starts there when
  // `after`, else on the one that ends there.
  Station station(double s, bool after) const {
    const int n = m_spine.edge_count();
    for (int i = 0; i < n; ++i) {
      const double start = m_spine.edge_start(i);
      const double end = start + m_spine.edge_length(i);
      if (after ? s < end - m_eps : s <= end + m_eps) {
        return {i, std::clamp(s - start, 0.0, m_spine.edge_length(i))};
      }
    }
    return {n - 1, m_spine.edge_length(n - 1)};
  }

  Bend bend(int edge) const { return m_bend[static_cast<std::size_t>(edge)]; }

  // The profile's frame at a station: as OCCT's constant binormal places
  // it, the binormal, and the normal binormal x tangent; for round
  // profiles any frame about the tangent; else the frame carried along the
  // spine.
  gp_Ax3 frame(const Station& at) const {
    const gp_Pnt p = m_spine.edge_point(at.edge, at.local);
    const gp_Dir t = m_spine.edge_tangent(at.edge, at.local);
    if (m_binormal) {
      const gp_Vec across = gp_Vec(*m_binormal).Crossed(gp_Vec(t));
      if (across.Magnitude() < 1.0e-9) {
        throw std::invalid_argument("the path runs along the sweep's binormal");
      }
      return gp_Ax3(p, *m_binormal, gp_Dir(across));
    }
    if (m_round) {
      return gp_Ax3(p, t, any_normal(t));
    }
    return carried(at.edge, m_starts[static_cast<std::size_t>(at.edge)], at.local);
  }

  // (mitcad#1708) The corrected Frenet frame along a spine of lines and
  // circular arcs, as OCCT's sweep carries it: along a line unturned,
  // along an arc turned about the arc's axis, and at a corner by the least
  // rotation that takes the arriving tangent to the leaving one (about
  // their cross product). `start` is the frame at the edge's start (its
  // main direction the tangent); `local` may be negative along a line or
  // an arc.
  gp_Ax3 carried(int edge, const gp_Ax3& start, double local) const {
    if (const std::optional<gp_Circ> circle = m_spine.edge_circle(edge)) {
      gp_Trsf turn;
      turn.SetRotation(circle->Axis(), local / circle->Radius());
      return start.Transformed(turn);
    }
    gp_Trsf move;
    move.SetTranslation(gp_Vec(start.Direction()) * local);
    return start.Transformed(move);
  }

  // The frame turned through a corner at `corner` from the tangent `in` to
  // `out`.
  static gp_Ax3 cornered(const gp_Ax3& frame, const gp_Pnt& corner, const gp_Dir& in, const gp_Dir& out) {
    const gp_Vec axis = gp_Vec(in).Crossed(gp_Vec(out));
    gp_Ax3 out_frame = frame;
    if (axis.Magnitude() > 1.0e-12) {
      gp_Trsf turn;
      turn.SetRotation(gp_Ax1(frame.Location(), gp_Dir(axis)), in.Angle(out));
      out_frame = frame.Transformed(turn);
    }
    out_frame.SetLocation(corner);
    return out_frame;
  }

  // The frames at the edges' starts, carried both ways from the profile's
  // frame at `at` (any frame about the tangent there: the profile is
  // placed relative to it). Throws where they are not known here: a curve
  // other than a line or a circular arc, or a closed spine swept round
  // whose frame does not come back to itself.
  void carry_frames(const Station& at) {
    const int n = m_spine.edge_count();
    for (int i = 0; i < n; ++i) {
      if (!m_spine.edge_straight(i) && !m_spine.edge_circle(i)) {
        throw std::invalid_argument("the frames along a curve other than a line or an arc are not known here");
      }
    }
    m_starts.assign(static_cast<std::size_t>(n), gp_Ax3());
    const gp_Dir t = m_spine.edge_tangent(at.edge, at.local);
    const gp_Ax3 here(m_spine.edge_point(at.edge, at.local), t, any_normal(t));
    gp_Ax3 start = carried(at.edge, here, -at.local);
    start.SetLocation(m_spine.edge_point(at.edge, 0.0));
    m_starts[static_cast<std::size_t>(at.edge)] = start;
    const auto end_of = [&](int i) {
      return carried(i, m_starts[static_cast<std::size_t>(i)], m_spine.edge_length(i));
    };
    for (int i = at.edge + 1; i < n; ++i) {
      m_starts[static_cast<std::size_t>(i)] =
          cornered(end_of(i - 1), m_spine.edge_point(i, 0.0), m_spine.edge_tangent(i - 1, m_spine.edge_length(i - 1)),
                   m_spine.edge_tangent(i, 0.0));
    }
    for (int i = at.edge - 1; i >= 0; --i) {
      const double length = m_spine.edge_length(i);
      const gp_Ax3 end = cornered(m_starts[static_cast<std::size_t>(i + 1)], m_spine.edge_point(i, length),
                                  m_spine.edge_tangent(i + 1, 0.0), m_spine.edge_tangent(i, length));
      gp_Ax3 back = carried(i, end, -length);
      back.SetLocation(m_spine.edge_point(i, 0.0));
      m_starts[static_cast<std::size_t>(i)] = back;
    }
    if (m_sweep.around) {
      const gp_Ax3 round = cornered(end_of(n - 1), m_spine.edge_point(0, 0.0),
                                    m_spine.edge_tangent(n - 1, m_spine.edge_length(n - 1)), m_spine.edge_tangent(0, 0.0));
      if (round.XDirection().Angle(m_starts.front().XDirection()) > 1.0e-9) {
        throw std::invalid_argument("the frame does not come back to itself round the closed path");
      }
    }
  }

  // The motion that takes the profile from where it lies to the station.
  gp_Trsf move_to(const Station& at) const {
    gp_Trsf move;
    move.SetDisplacement(m_at, frame(at));
    return move;
  }

  // The profile's points at the station, from the path's point there.
  std::vector<gp_Vec> offsets(const Station& at) const {
    const gp_Trsf move = move_to(at);
    const gp_Pnt origin = m_spine.edge_point(at.edge, at.local);
    std::vector<gp_Vec> out;
    out.reserve(m_samples.size());
    for (const gp_Pnt& p : m_samples) {
      out.emplace_back(origin, p.Transformed(move));
    }
    return out;
  }

  // How far the profile reaches along `direction` at the station.
  double extent(const Station& at, const gp_Vec& direction) const {
    double most = -std::numeric_limits<double>::infinity();
    for (const gp_Vec& x : offsets(at)) {
      most = std::max(most, x.Dot(direction));
    }
    return most;
  }

  std::vector<Joint> joints() const {
    std::vector<Joint> out;
    const int n = m_spine.edge_count();
    const auto joint = [&](int before, int after, double s) {
      Joint j;
      j.s = s;
      j.before = before;
      j.after = after;
      j.in = m_spine.edge_tangent(before, m_spine.edge_length(before));
      j.out = m_spine.edge_tangent(after, 0.0);
      j.turn = j.in.Angle(j.out);
      out.push_back(j);
    };
    for (int i = 1; i < n; ++i) {
      joint(i - 1, i, m_spine.edge_start(i));
    }
    if (m_sweep.around) {
      joint(n - 1, 0, 0.0);
    }
    return out;
  }

  // A corner whose joint is more than a sliver within the tolerance.
  bool sharp(const Joint& joint) const {
    return m_reach * std::tan(std::min(joint.turn, kTurnBack) / 2.0) > m_sweep.tolerance;
  }

  bool mitred(const Joint& joint) const { return sharp(joint) && m_sweep.corners == PipeCorners::Mitered; }

  bool cut_at(const std::vector<double>& cuts, double s) const {
    return std::any_of(cuts.begin(), cuts.end(), [&](double c) { return std::abs(c - s) <= m_eps; });
  }

  void add_cut(std::vector<double>& cuts, double s) const {
    if (!cut_at(cuts, s)) {
      cuts.push_back(s);
      std::sort(cuts.begin(), cuts.end());
    }
  }

  const Joint* joint_at(double s) const {
    for (const Joint& joint : m_joints) {
      if (std::abs(joint.s - s) <= m_eps || (m_sweep.around && joint.s <= m_eps && s >= m_length - m_eps)) {
        return &joint;
      }
    }
    return nullptr;
  }

  // The arcs the profile reaches past the centre of (turned in pieces),
  // the other curves it reaches past the centre of a bend of: (mitcad#1709)
  // under a round profile in one plane, a curve whose bend is tightest at
  // one point inside it is built from its tube (Bend::Creased, see
  // add_creased_faces), and in no plane the whole tube less where it meets
  // itself (Bend::Slotted, see add_slotted_faces, which throws where it is
  // not built so); other folds are not resolved here (mitcad#1708: along a
  // curve whose bend changes the line through the centre of the bend moves
  // across the profile, so no cut of the profile keeps each side from
  // folding; along a helix it stays put only where the frames follow its
  // screw motion, and the surface it sweeps touches the profile's plane at
  // each end of the piece, a knife edge that OCCT's union of the sides
  // does not build reliably).
  void find_bends() {
    const int n = m_spine.edge_count();
    m_bend.assign(static_cast<std::size_t>(n), Bend::None);
    for (int i = 0; i < n; ++i) {
      if (const std::optional<gp_Circ> circle = m_spine.edge_circle(i)) {
        const Station start{i, 0.0};
        const gp_Pnt p = m_spine.edge_point(i, 0.0);
        const double radius = circle->Radius();
        const gp_Vec inward = gp_Vec(p, circle->Location()) / radius;
        if (extent(start, inward) > radius + m_slack) {
          m_bend[static_cast<std::size_t>(i)] = Bend::Turned;
          m_folds = true;
          note_where(p);
          // Turned about the arc's axis only where the frames turn about
          // it: a constant binormal along it, the corrected Frenet frame
          // (along a circle it turns about its axis), or a round profile.
          if (m_binormal && !m_round && std::abs(circle->Axis().Direction().Dot(*m_binormal)) < 1.0 - 1.0e-9) {
            m_unresolvable = true;
          }
        }
        continue;
      }
      if (m_spine.edge_straight(i)) {
        continue;
      }
      const double length = m_spine.edge_length(i);
      for (int k = 0; k <= kBendSamples; ++k) {
        const auto curving = m_spine.edge_bend(i, length * k / kBendSamples);
        if (curving) {
          m_tightest = std::min(m_tightest, 1.0 / curving->first);
        }
      }
      for (int k = 0; k <= kBendSamples; ++k) {
        const Station at{i, length * k / kBendSamples};
        const auto curving = m_spine.edge_bend(i, at.local);
        if (curving && extent(at, gp_Vec(curving->second)) * curving->first > 1.0 + 1.0e-9) {
          m_folds = true;
          note_where(m_spine.edge_point(i, at.local));
          // (mitcad#1709) A round profile in one plane: from its tube.
          std::optional<Crease> crease;
          if (m_round && m_binormal && m_radii.size() <= 2 &&
              (crease = crease_of(PlaneCurve(m_spine.edge_curve(i), *m_binormal), m_radii.front())) &&
              (m_radii.size() == 1 || m_radii.back() < crease->bend * (1.0 - 1.0e-9))) {
            m_bend[static_cast<std::size_t>(i)] = Bend::Creased;
            m_creases.emplace(i, *crease);
          } else if (m_round && !m_binormal && m_radii.size() <= 2) {
            // In no plane: the whole tube less its slot, where it builds.
            m_bend[static_cast<std::size_t>(i)] = Bend::Slotted;
          } else {
            m_unresolvable = true;
          }
          break;
        }
      }
    }
  }

  // How far the profile's side on the straight edge leans back from a
  // mitred corner per unit of its offset across the edge (mitcad#1497): the
  // other leg's direction across the edge over 1 + cos of the turn.
  static gp_Vec lean(const gp_Dir& other, const gp_Dir& along) {
    const double cos = other.Dot(along);
    return (gp_Vec(other) - gp_Vec(along) * cos) / (1.0 + cos);
  }

  // The corners where a straight piece between the cuts and the joints is
  // too short for the miters at its ends: its sides would end before they
  // start. A miter at the piece's start puts its side at x across the edge
  // back to x . lean_start from the corner, one at its end at
  // length - x . lean_end.
  std::vector<double> folded_legs(const std::vector<double>& cuts) const {
    std::vector<double> more = cuts;
    std::vector<double> added;
    bool again = true;
    while (again) {
      again = false;
      for (int i = 0; i < m_spine.edge_count(); ++i) {
        if (bend(i) != Bend::None) {
          continue;
        }
        const bool straight = m_spine.edge_straight(i);
        const double start = m_spine.edge_start(i);
        const double end = start + m_spine.edge_length(i);
        const gp_Dir along = m_spine.edge_tangent(i, 0.0);
        std::vector<double> ends{start};
        for (const double c : more) {
          if (c > start + m_eps && c < end - m_eps) {
            ends.push_back(c);
          }
        }
        ends.push_back(end);
        for (std::size_t k = 0; k + 1 < ends.size(); ++k) {
          const double from = ends[k];
          const double to = ends[k + 1];
          const Joint* before = k == 0 && !cut_at(more, from) ? joint_at(from) : nullptr;
          const Joint* after = k + 2 == ends.size() && !cut_at(more, to) ? joint_at(to) : nullptr;
          if (before != nullptr && !mitred(*before)) {
            before = nullptr;
          }
          if (after != nullptr && !mitred(*after)) {
            after = nullptr;
          }
          if (before == nullptr && after == nullptr) {
            continue;
          }
          if ((before != nullptr && before->turn > kTurnBack) || (after != nullptr && after->turn > kTurnBack)) {
            continue;
          }
          if (straight) {
            const gp_Vec lean_start = before != nullptr ? lean(before->in, along) : gp_Vec();
            const gp_Vec lean_end = after != nullptr ? lean(after->out, along) : gp_Vec();
            if (to - from >= extent({i, 0.0}, lean_end - lean_start) - m_slack) {
              continue;
            }
          } else if (!curve_too_short(i, from - start, to - start, before, after)) {
            continue;
          }
          for (const Joint* joint : {before, after}) {
            if (joint != nullptr && !cut_at(more, joint->s)) {
              add_cut(more, joint->s);
              added.push_back(joint->s);
              again = true;
            }
          }
        }
      }
    }
    return added;
  }

  // (mitcad#1708) Whether a curved piece of the edge, from `from` to `to`
  // along it, is too short for the miters at its ends: some point of the
  // profile runs along the piece less far than the miters cut into it (as
  // on a straight piece, at x across the edge a miter at the start cuts in
  // by -x . lean_start, one at the end by x . lean_end, with the tangents
  // and the profile's place at the piece's ends), so that its side would
  // end before it starts. How far a point runs is summed over short steps
  // along the tangent.
  bool curve_too_short(int edge, double from, double to, const Joint* before, const Joint* after) const {
    const int n = kBendSamples;
    std::vector<std::vector<gp_Vec>> offset(static_cast<std::size_t>(n) + 1);
    std::vector<gp_Pnt> point(static_cast<std::size_t>(n) + 1);
    std::vector<gp_Dir> tangent(static_cast<std::size_t>(n) + 1);
    for (int k = 0; k <= n; ++k) {
      const Station at{edge, from + (to - from) * k / n};
      offset[static_cast<std::size_t>(k)] = offsets(at);
      point[static_cast<std::size_t>(k)] = m_spine.edge_point(edge, at.local);
      tangent[static_cast<std::size_t>(k)] = m_spine.edge_tangent(edge, at.local);
    }
    const gp_Vec lean_start = before != nullptr ? lean(before->in, tangent.front()) : gp_Vec();
    const gp_Vec lean_end = after != nullptr ? lean(after->out, tangent.back()) : gp_Vec();
    for (std::size_t j = 0; j < m_samples.size(); ++j) {
      double run = 0.0;
      for (std::size_t k = 0; k + 1 < point.size(); ++k) {
        const gp_Pnt a = point[k].Translated(offset[k][j]);
        const gp_Pnt b = point[k + 1].Translated(offset[k + 1][j]);
        const gp_Vec middle = gp_Vec(tangent[k]) + gp_Vec(tangent[k + 1]);
        if (middle.Magnitude() > 1.0e-12) {
          run += gp_Vec(a, b).Dot(middle.Normalized());
        }
      }
      const double cut_start = -offset.front()[j].Dot(lean_start);
      const double cut_end = offset.back()[j].Dot(lean_end);
      if (run - cut_start - cut_end < -m_slack) {
        return true;
      }
    }
    return false;
  }

  void note_where(const gp_Pnt& p) {
    if (!m_placed) {
      m_where = p;
      m_placed = true;
    }
  }

  // Where the path comes back near itself (mitcad#161's overlap_cuts):
  // the shortest stretch of it between two of its points farther apart
  // along it than four reaches of the profile but within two of each
  // other.
  void find_return() {
    const double length = m_length;
    if (m_reach <= 0.0 || length <= 4.0 * m_reach) {
      return;
    }
    const double step = std::max(std::min(m_reach / 2.0, length / 16.0), length / 4000.0);
    const int n = static_cast<int>(std::ceil(length / step));
    std::vector<gp_Pnt> points;
    for (int i = 0; i <= n; ++i) {
      const Station at = station(length * i / n, true);
      points.push_back(m_spine.edge_point(at.edge, at.local));
    }
    for (int i = 0; i <= n; ++i) {
      for (int j = i + 1; j <= n; ++j) {
        double along = length * (j - i) / n;
        if (m_sweep.around) {
          along = std::min(along, length - along);
        }
        if (along > 4.0 * m_reach && along < m_returning && points[i].Distance(points[j]) < 2.0 * m_reach) {
          m_returning = along;
          note_where(points[i]);
        }
      }
    }
  }

  // How far from a corner at an end of the edge a cut inside it must lie
  // so that the piece between them is not too short for the corner's
  // miter.
  double margin(int edge, const Joint* joint, bool at_start) const {
    if (joint == nullptr || !mitred(*joint)) {
      return 0.0;
    }
    if (!m_spine.edge_straight(edge)) {
      return m_reach * std::tan(std::min(joint->turn, kTurnBack) / 2.0);
    }
    const gp_Dir along = m_spine.edge_tangent(edge, 0.0);
    const gp_Vec reach = at_start ? lean(joint->in, along).Reversed() : lean(joint->out, along);
    return std::max(0.0, extent({edge, 0.0}, reach)) + m_slack;
  }

  // Cuts between the given ones, so that no piece is as long as the
  // shortest stretch along which the path comes back near itself: at
  // joints of the path, or inside its edges far enough from their
  // corners. Never inside an arc that is turned in pieces.
  std::vector<double> returning_cuts(const std::vector<double>& cuts) const {
    const double longest = 0.9 * m_returning;
    std::vector<double> places;
    for (int i = 0; i < m_spine.edge_count(); ++i) {
      const double start = m_spine.edge_start(i);
      const double end = start + m_spine.edge_length(i);
      places.push_back(start);
      places.push_back(end);
      if (bend(i) != Bend::None) {
        continue;
      }
      const double from = start + margin(i, joint_at(start), true);
      const double to = end - margin(i, joint_at(end), false);
      if (to <= from) {
        continue;
      }
      const int steps = static_cast<int>(std::ceil((to - from) / (0.05 * longest)));
      for (int k = 0; k <= steps; ++k) {
        places.push_back(from + (to - from) * k / std::max(1, steps));
      }
    }
    std::sort(places.begin(), places.end());
    std::vector<double> bounds{0.0};
    bounds.insert(bounds.end(), cuts.begin(), cuts.end());
    bounds.push_back(m_length);
    std::sort(bounds.begin(), bounds.end());
    std::vector<double> added;
    for (std::size_t k = 0; k + 1 < bounds.size(); ++k) {
      double at = bounds[k];
      const double to = bounds[k + 1];
      const Station piece = station(at, true);
      if (bend(piece.edge) != Bend::None && piece.local <= m_eps) {
        continue;
      }
      while (to - at > longest) {
        double next = at;
        for (const double p : places) {
          if (p > at + m_eps && p < to - m_eps && p - at <= longest) {
            next = p;
          }
        }
        if (next <= at) {
          throw std::runtime_error("no place to cut the path where it comes back near itself");
        }
        added.push_back(next);
        at = next;
      }
    }
    return added;
  }

  // The joint of the pieces cut at a corner: mitred, the miter beyond the
  // two pieces' ends (OCCT's miter of the section along the two directions
  // a little way on, within the wedge between the planes square to them at
  // the corner); rounded, the section turned about the corner.
  std::vector<ShapePtr> joined(const SweptProfile& profile, const Joint& joint) const {
    const double s = joint.s <= m_eps && m_sweep.around ? m_length : joint.s;
    const Station end = station(s, false);
    const std::vector<NamedWire> loops = placed(profile.loops, move_to(end));
    const gp_Pnt corner = m_spine.edge_point(end.edge, end.local);
    if (joint.turn > kTurnBack) {
      throw std::runtime_error(std::string(m_sweep.what) + ": the path turns right back at a corner");
    }
    const gp_Vec axis = gp_Vec(joint.in).Crossed(gp_Vec(joint.out));
    if (m_sweep.corners == PipeCorners::Rounded) {
      return turned(loops, gp_Ax1(corner, gp_Dir(axis)), joint.turn, {}, {}, m_sweep.what);
    }
    const double run = 1.2 * m_reach * std::tan(joint.turn / 2.0) + 0.1 * m_reach + 10.0 * m_sweep.tolerance;
    gp_Trsf back;
    back.SetTranslation(gp_Vec(joint.in) * -run);
    const std::vector<NamedWire> before = placed(loops, back);
    const gp_Pnt a = corner.Translated(gp_Vec(joint.in) * -run);
    const gp_Pnt b = corner.Translated(gp_Vec(joint.out) * run);
    BRepBuilderAPI_MakeWire wire(BRepBuilderAPI_MakeEdge(a, corner).Edge(), BRepBuilderAPI_MakeEdge(corner, b).Edge());
    if (!wire.IsDone()) {
      throw std::runtime_error(std::string(m_sweep.what) + ": the corner's miter could not be built");
    }
    const ShapePtr outer = pipe_shell(before.front(), wire.Wire(), m_sweep.mode, {}, {}, m_sweep.tolerance,
                                      PipeCorners::Mitered);
    std::vector<ShapePtr> holes;
    for (std::size_t l = 1; l < before.size(); ++l) {
      holes.push_back(pipe_shell(before[l], wire.Wire(), m_sweep.mode, {}, {}, m_sweep.tolerance,
                                 PipeCorners::Mitered));
    }
    const ShapePtr miter = with_holes(outer, holes, m_sweep.what);
    BRepAlgoAPI_Common common;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(miter->occt());
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(corner_wedge(corner, joint.in, joint.out, 4.0 * (run + m_reach)));
    common.SetArguments(arguments);
    common.SetTools(tools);
    common.SetNonDestructive(true);
    build(common);
    if (!common.IsDone() || common.HasErrors()) {
      throw std::runtime_error(std::string(m_sweep.what) + ": the corner's miter could not be cut out");
    }
    FaceNamer namer(common.Shape());
    namer.carry(common, *miter);
    return {namer.shape()};
  }

  const FoldedSweep& m_sweep;
  const Spine& m_spine;
  const std::vector<SweptProfile>& m_profiles;
  std::optional<gp_Dir> m_binormal;
  bool m_round = false;
  // (mitcad#1709) A round profile's radii (the rim, then the bore's) and
  // the creases of the bends built from their tube.
  std::vector<double> m_radii;
  std::map<int, Crease> m_creases;
  // The radius of the tightest bend of the curves sampled for folds.
  double m_tightest = std::numeric_limits<double>::infinity();
  std::vector<gp_Ax3> m_starts;
  double m_length = 0.0;
  double m_eps = 0.0;
  double m_slack = 0.0;
  gp_Ax3 m_at;
  std::vector<gp_Pnt> m_samples;
  double m_reach = 0.0;
  std::vector<Joint> m_joints;
  std::vector<Bend> m_bend;
  std::vector<double> m_cuts;
  bool m_folds = false;
  bool m_unresolvable = false;
  bool m_turns_back = false;
  gp_Pnt m_back;
  double m_returning = std::numeric_limits<double>::infinity();
  gp_Pnt m_where;
  bool m_placed = false;
  const NameList m_none;
};

// The refusal of a sweep that folds and is not built: what is wrong in
// words, where it folds as its place.
std::string refusal(const FoldedSweep& sweep, const gp_Pnt& where, const std::string& why) {
  return placed_error(std::string(sweep.what) +
                          " passes through itself along its path and could not be built in pieces there",
                      std::string(sweep.what) + " passes through itself; in pieces: " + why, {point_place(where)});
}

// The sweep in one piece: the shape and whether it is sound, or the error
// that stopped it.
struct Whole {
  ShapePtr shape;
  bool sound = false;
  std::exception_ptr error;

  // The shape as it is (unsound: the caller's check refuses it), or the
  // error again.
  UnfoldedSweep as_built() const {
    if (!shape) {
      std::rethrow_exception(error);
    }
    return {shape, false};
  }
};

Whole built_whole(const std::function<ShapePtr()>& whole) {
  Whole out;
  try {
    out.shape = whole();
    out.sound = sound(*out.shape);
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    throw_if_cancelled();
    out.shape = nullptr;
    out.error = std::current_exception();
  }
  return out;
}

// The normal of the plane of a spine of straight edges that turns in one
// plane; none for another spine. Along it the corrected Frenet frame keeps
// the section's angle to that normal, as a constant binormal does: its
// lines keep the frame, and it turns at a corner about the corner's axis,
// which is the normal.
std::optional<gp_Dir> polyline_normal(const Spine& spine) {
  std::optional<gp_Dir> normal;
  const int n = spine.edge_count();
  for (int i = 0; i < n; ++i) {
    if (!spine.edge_straight(i)) {
      return std::nullopt;
    }
  }
  for (int i = 1; i < n && !normal; ++i) {
    const gp_Vec cross = gp_Vec(spine.edge_tangent(i - 1, 0.0)).Crossed(gp_Vec(spine.edge_tangent(i, 0.0)));
    if (cross.Magnitude() > 1.0e-9) {
      normal = gp_Dir(cross);
    }
  }
  if (!normal) {
    return std::nullopt;
  }
  const gp_Pnt origin = spine.edge_point(0, 0.0);
  for (int i = 0; i < n; ++i) {
    const gp_Pnt end = spine.edge_point(i, spine.edge_length(i));
    if (std::abs(gp_Vec(origin, end).Dot(gp_Vec(*normal))) > 1.0e-7) {
      return std::nullopt;
    }
  }
  return normal;
}

} // namespace

UnfoldedSweep swept_resolving_folds(const FoldedSweep& sweep, const std::vector<SweptProfile>& profiles,
                                    const std::function<ShapePtr()>& whole) {
  if (sweep.spine == nullptr || profiles.empty()) {
    return {whole(), false};
  }
  // The frames are known here along a constant binormal, along a polyline
  // in one plane, and (mitcad#1708) carried along a spine of lines and
  // circular arcs in no plane, or any frame for a round profile.
  std::optional<gp_Dir> binormal;
  if (sweep.mode.kind == PipeMode::Kind::Binormal) {
    binormal = sweep.mode.binormal;
  } else if (sweep.mode.kind == PipeMode::Kind::CorrectedFrenet) {
    binormal = polyline_normal(*sweep.spine);
  } else {
    return {whole(), false};
  }
  std::optional<Unfolder> unfolder;
  try {
    unfolder.emplace(sweep, profiles, binormal);
  } catch (const Cancelled&) {
    throw;
  } catch (const std::exception&) {
    // Frames not known here (a path along the binormal, a profile at a
    // corner, a curve in no plane under a profile that is not round): as
    // before.
    throw_if_cancelled();
    return {whole(), false};
  }
  const auto in_pieces = [&](bool at_corners, std::string& why) -> ShapePtr {
    try {
      if (unfolder->tube_only()) {
        return unfolder->whole_tube();
      }
      return unfolder->pieces_fused(unfolder->plan(at_corners));
    } catch (const Cancelled&) {
      throw;
    } catch (const std::exception& error) {
      throw_if_cancelled();
      why = error.what();
    }
    return nullptr;
  };
  std::string why;
  if (unfolder->folds() && !unfolder->unresolvable()) {
    if (ShapePtr shape = in_pieces(false, why)) {
      return {shape, true};
    }
  }
  if (unfolder->folds()) {
    // Not resolved here: the sweep in one piece where it builds (the model
    // warns of the fold), else the refusal.
    const Whole one = built_whole(whole);
    if (one.sound) {
      return {one.shape, false};
    }
    if (const std::optional<gp_Pnt> back = unfolder->turning_back()) {
      // (mitcad#1708) No miter can be built where the path turns right
      // back on itself.
      throw std::runtime_error(placed_error(
          std::string(sweep.what) + "'s path turns right back at a corner, where no mitered corner can be built",
          std::string(sweep.what) + " passes through itself where its path turns right back at a corner",
          {point_place(*back)}));
    }
    if (why.empty()) {
      why = "the fold is not of a kind built in pieces";
    }
    throw std::runtime_error(refusal(sweep, unfolder->where(), why));
  }
  if (unfolder->returns()) {
    // The path comes back near itself: a fold only where the sides cross.
    const Whole one = built_whole(whole);
    if (one.sound && !self_interfering(*one.shape)) {
      return {one.shape, false};
    }
    if (!unfolder->unresolvable()) {
      if (ShapePtr pieces = in_pieces(false, why)) {
        return {pieces, true};
      }
    }
    return one.as_built();
  }
  // No fold: in one piece; where that does not build, in pieces at the
  // corners.
  const Whole one = built_whole(whole);
  if (one.sound) {
    return {one.shape, false};
  }
  if (unfolder->has_corner()) {
    if (ShapePtr pieces = in_pieces(true, why)) {
      return {pieces, false};
    }
  }
  return one.as_built();
}

} // namespace mitcad::geometry::detail
