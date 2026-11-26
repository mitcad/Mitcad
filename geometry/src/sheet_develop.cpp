// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sheet_develop.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Circle.hxx>
#include <Geom2d_Line.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2d.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>

#include "mitcad/geometry/datum.hpp"
#include "sheet_curve.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;
// Edges that are no generator and no circle about the axis: cubic
// B-splines through their exact development within this (mm), as an
// emboss wraps a profile onto a cone (mitcad#473).
constexpr double kFit = 1.0e-8;
// Faces lie on one surface within this (mm), as face_geometry recognises
// surfaces stored as other types.
constexpr double kSame = 1.0e-5;
// An edge runs along a generator or round the axis when its development
// keeps one chart coordinate within this (mm).
constexpr double kOnLine = 1.0e-7;
// Points of each edge read for its angle round the axis and its kind.
constexpr int kSamples = 256;

// The cylinder or cone a face lies on: a cylinder's axis point, axis and
// radius; a cone's apex, its axis where the radius grows and half angle.
struct Support {
  bool cone = false;
  gp_Pnt origin;
  gp_Dir axis{0, 0, 1};
  double radius = 0;
  double half_angle = 0;
};

Support support_of(const TopoDS_Face& face) {
  const occ::handle<Geom_Surface> analytic = analytic_surface(face);
  if (analytic.IsNull()) throw std::invalid_argument("the face is not a cylinder or a cone");
  const GeomAdaptor_Surface surface(analytic);
  Support result;
  if (surface.GetType() == GeomAbs_Cylinder) {
    const gp_Cylinder cylinder = surface.Cylinder();
    result.origin = cylinder.Location();
    result.axis = cylinder.Axis().Direction();
    result.radius = cylinder.Radius();
  } else if (surface.GetType() == GeomAbs_Cone) {
    const gp_Cone cone = surface.Cone();
    result.cone = true;
    result.origin = cone.Apex();
    result.axis = cone.SemiAngle() > 0 ? cone.Axis().Direction() : cone.Axis().Direction().Reversed();
    result.half_angle = std::abs(cone.SemiAngle());
  } else {
    throw std::invalid_argument("the face is not a cylinder or a cone");
  }
  return result;
}

// The face lies on the support: points of its own surface over its
// parameter box within kSame of it (faces stored as splines are each a
// cylinder or a cone within 1e-6 mm, recognised one by one, so their
// recognised axes and radii differ by more than that over the panel).
bool on_support(const TopoDS_Face& face, const Support& support) {
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface surface(face);
  const gp_Vec axis(support.axis);
  for (int i = 0; i <= 4; ++i) {
    for (int j = 0; j <= 4; ++j) {
      const gp_Vec d(support.origin, surface.Value(u0 + (u1 - u0) * i / 4, v0 + (v1 - v0) * j / 4));
      const double h = d.Dot(axis);
      const double r = (d - h * axis).Magnitude();
      const double off = support.cone ? r * std::cos(support.half_angle) - h * std::sin(support.half_angle)
                                      : r - support.radius;
      if (std::abs(off) > kSame) return false;
    }
  }
  return true;
}

double wrapped(double angle) {
  while (angle > kPi) angle -= 2 * kPi;
  while (angle <= -kPi) angle += 2 * kPi;
  return angle;
}

// An edge as a face's wire runs it: its points at parameters increasing
// over its range, their angles round the axis as read (`raw`) and as the
// wire runs on round it (`angle`, continuous along the wire).
struct Track {
  TopoDS_Edge edge;
  double first = 0, last = 0;
  std::vector<double> params;
  std::vector<gp_Pnt> points;
  std::vector<double> raw, angle;

  double middle() const { return angle[angle.size() / 2]; }
};

struct FaceTracks {
  std::vector<Track> tracks;
  double shift = 0;
  bool placed = false;
};

// A cubic B-spline through `exact` at evenly spaced parameters of
// [first, last], more of them until it is within kFit of it between them.
occ::handle<Geom2d_BSplineCurve> fitted(const std::function<gp_Pnt2d(double)>& exact, double first, double last) {
  for (int n = 16; n <= 4096; n *= 2) {
    throw_if_cancelled();
    const occ::handle<NCollection_HArray1<gp_Pnt2d>> points = new NCollection_HArray1<gp_Pnt2d>(1, n + 1);
    const occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n + 1);
    for (int i = 0; i <= n; ++i) {
      const double t = first + (last - first) * i / n;
      points->SetValue(i + 1, exact(t));
      params->SetValue(i + 1, t);
    }
    Geom2dAPI_Interpolate interpolate(points, params, false, 1.0e-12);
    interpolate.Perform();
    if (!interpolate.IsDone()) break;
    const occ::handle<Geom2d_BSplineCurve> curve = interpolate.Curve();
    double worst = 0;
    for (int i = 0; i < n; ++i) {
      const double t = first + (last - first) * (i + 0.5) / n;
      worst = std::max(worst, curve->Value(t).Distance(exact(t)));
    }
    if (worst <= kFit) return curve;
  }
  throw std::runtime_error("an edge of the face could not be developed within 1e-8 mm");
}

BoundedCurve2 chart_line(const gp_Pnt2d& a, const gp_Pnt2d& b) {
  return detail::export_sheet_curve(new Geom2d_Line(a, gp_Dir2d(gp_Vec2d(a, b))), {0.0, a.Distance(b)});
}

} // namespace

SheetFaceDevelopment sheet_face_development(const Shape& shape, const std::vector<std::string>& names,
                                            double neutral_depth, double thickness) {
  return detail::run("sheet face development", [&] {
    detail::require_finite("the neutral surface's depth", neutral_depth);
    detail::require_positive("the sheet's thickness", thickness);
    std::vector<TopoDS_Face> faces;
    for (const std::string& name : names) {
      const std::vector<int> found = shape.find_faces(name);
      if (found.empty()) throw std::invalid_argument("the body has no such face");
      for (const int f : found) {
        const TopoDS_Face face = shape.face(f);
        const bool listed = std::any_of(faces.begin(), faces.end(), [&](const TopoDS_Face& g) { return g.IsSame(face); });
        if (!listed) faces.push_back(face);
      }
    }
    if (faces.empty()) throw std::invalid_argument("no face to develop");
    const Support support = support_of(faces.front());
    for (const TopoDS_Face& face : faces) {
      if (!on_support(face, support)) {
        throw std::invalid_argument("the faces do not lie on one cylinder or cone");
      }
    }
    SheetFaceDevelopment result;
    result.cone = support.cone;
    // The way the first face's material lies: its outward normal towards
    // the axis makes it the sheet's inner surface.
    {
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(faces.front(), u0, u1, v0, v1);
      const BRepAdaptor_Surface surface(faces.front());
      BRepLProp_SLProps props(surface, (u0 + u1) / 2, (v0 + v1) / 2, 1, 1.0e-9);
      if (!props.IsNormalDefined()) throw std::runtime_error("the face has no normal at its middle");
      gp_Dir normal = props.Normal();
      if (faces.front().Orientation() == TopAbs_REVERSED) normal.Reverse();
      gp_Vec radial(support.origin, props.Value());
      radial -= radial.Dot(gp_Vec(support.axis)) * gp_Vec(support.axis);
      result.inner_is_front = gp_Vec(normal).Dot(radial) < 0;
    }
    // The neutral surface from the faces', away from the axis.
    const double offset = result.inner_is_front ? neutral_depth : -(thickness - neutral_depth);
    const gp_Vec axis(support.axis);
    const double sin_angle = std::sin(support.half_angle), cos_angle = std::cos(support.half_angle);
    if (support.cone) {
      result.half_angle = support.half_angle;
      result.origin = support.origin.Translated(axis * (-offset / sin_angle));
    } else {
      result.origin = support.origin;
      result.neutral_radius = support.radius + offset;
      detail::require_positive("the neutral surface's radius", result.neutral_radius);
    }
    result.axis = support.axis;
    // A frame across the axis to read angles round it.
    gp_Vec x(1, 0, 0);
    if (std::abs(x.Dot(axis)) > 0.9) x = gp_Vec(0, 1, 0);
    x = (x - x.Dot(axis) * axis).Normalized();
    const gp_Vec y = axis.Crossed(x);
    const auto raw_angle = [&](const gp_Pnt& p) {
      const gp_Vec d(support.origin, p);
      return std::atan2(d.Dot(y), d.Dot(x));
    };
    // Every face's edges, the angles continuous along each wire; a hole's
    // wire turned by whole turns to lie with its face's outline.
    std::vector<FaceTracks> tracked(faces.size());
    for (std::size_t f = 0; f < faces.size(); ++f) {
      const TopoDS_Wire outer = BRepTools::OuterWire(faces[f]);
      std::vector<TopoDS_Wire> wires{outer};
      for (TopExp_Explorer w(faces[f], TopAbs_WIRE); w.More(); w.Next()) {
        if (!w.Current().IsSame(outer)) wires.push_back(TopoDS::Wire(w.Current()));
      }
      double outer_mean = 0;
      for (std::size_t w = 0; w < wires.size(); ++w) {
        const std::size_t begin = tracked[f].tracks.size();
        bool started = false;
        double previous_raw = 0, previous_angle = 0;
        for (BRepTools_WireExplorer e(wires[w], faces[f]); e.More(); e.Next()) {
          throw_if_cancelled();
          const TopoDS_Edge edge = e.Current();
          if (BRep_Tool::Degenerated(edge)) {
            if (support.cone) {
              throw std::invalid_argument("the face runs to its cone's apex, which no sheet can be formed to");
            }
            continue;
          }
          const BRepAdaptor_Curve curve(edge);
          Track track;
          track.edge = edge;
          track.first = curve.FirstParameter();
          track.last = curve.LastParameter();
          for (int i = 0; i <= kSamples; ++i) {
            const double t = track.first + (track.last - track.first) * i / kSamples;
            track.params.push_back(t);
            track.points.push_back(curve.Value(t));
            track.raw.push_back(raw_angle(track.points.back()));
          }
          track.angle.assign(track.raw.size(), 0);
          const bool reversed = edge.Orientation() == TopAbs_REVERSED;
          for (int k = 0; k <= kSamples; ++k) {
            const int i = reversed ? kSamples - k : k;
            if (!started) {
              previous_raw = previous_angle = track.raw[i];
              started = true;
            }
            track.angle[i] = previous_angle + wrapped(track.raw[i] - previous_raw);
            previous_raw = track.raw[i];
            previous_angle = track.angle[i];
          }
          tracked[f].tracks.push_back(std::move(track));
        }
        double sum = 0;
        std::size_t count = 0;
        for (std::size_t t = begin; t < tracked[f].tracks.size(); ++t) {
          for (const double a : tracked[f].tracks[t].angle) {
            sum += a;
            ++count;
          }
        }
        const double mean = count > 0 ? sum / count : 0;
        if (w == 0) {
          outer_mean = mean;
        } else {
          const double turn = 2 * kPi * std::round((outer_mean - mean) / (2 * kPi));
          for (std::size_t t = begin; t < tracked[f].tracks.size(); ++t) {
            for (double& a : tracked[f].tracks[t].angle) a += turn;
          }
        }
      }
    }
    // The faces turned by whole turns to meet at their shared edges, from
    // the first; where they meet again further round, they stay apart (the
    // seam of faces closing round the axis).
    tracked.front().placed = true;
    for (bool progress = true; progress;) {
      progress = false;
      for (auto& face : tracked) {
        if (face.placed) continue;
        for (const auto& other : tracked) {
          if (!other.placed || face.placed) continue;
          for (const auto& mine : face.tracks) {
            const auto shared = std::find_if(other.tracks.begin(), other.tracks.end(),
                                             [&](const Track& t) { return t.edge.IsSame(mine.edge); });
            if (shared == other.tracks.end()) continue;
            face.shift = 2 * kPi * std::round((shared->middle() + other.shift - mine.middle()) / (2 * kPi));
            face.placed = true;
            progress = true;
            break;
          }
        }
      }
    }
    for (const auto& face : tracked) {
      if (!face.placed) throw std::invalid_argument("the faces on the cylinder or cone are not joined");
    }
    double low = 1e300, high = -1e300;
    for (const auto& face : tracked) {
      for (const auto& track : face.tracks) {
        for (const double a : track.angle) {
          low = std::min(low, a + face.shift);
          high = std::max(high, a + face.shift);
        }
      }
    }
    if (!(high >= low)) throw std::invalid_argument("the faces have no edges to develop");
    result.turn = high - low;
    if (result.turn > 2 * kPi + 1.0e-7) throw std::invalid_argument("the faces run round their axis more than once");
    result.closed = result.turn >= 2 * kPi - 1.0e-7;
    if (result.closed) result.turn = 2 * kPi;
    result.radial_start = gp_Dir(std::cos(low) * x + std::sin(low) * y);
    result.angular_direction = gp_Dir(-std::sin(low) * x + std::cos(low) * y);
    // A point at the angle `angle` from the start into the chart.
    const auto chart = [&](const gp_Pnt& p, double angle) {
      if (!support.cone) {
        return gp_Pnt2d(result.neutral_radius * angle, gp_Vec(result.origin, p).Dot(axis));
      }
      const gp_Vec d(result.origin, p);
      const double h = d.Dot(axis);
      const double r = (d - h * axis).Magnitude();
      const double rho = r * sin_angle + h * cos_angle;
      const double phi = sin_angle * angle;
      return gp_Pnt2d(rho * std::cos(phi), rho * std::sin(phi));
    };
    for (const auto& face : tracked) {
      std::vector<BoundedCurve2> edges;
      for (const auto& track : face.tracks) {
        throw_if_cancelled();
        const double turned = face.shift - low;
        std::vector<gp_Pnt2d> q;
        for (std::size_t i = 0; i < track.points.size(); ++i) q.push_back(chart(track.points[i], track.angle[i] + turned));
        double spread_angle = 0, spread_along = 0, length = 0;
        for (std::size_t i = 1; i < q.size(); ++i) length += q[i].Distance(q[i - 1]);
        if (length <= 1.0e-9) continue;
        for (std::size_t i = 0; i < q.size(); ++i) {
          if (!support.cone) {
            spread_angle = std::max(spread_angle, std::abs(q[i].X() - q[0].X()));
            spread_along = std::max(spread_along, std::abs(q[i].Y() - q[0].Y()));
          } else {
            const double rho = std::hypot(q[i].X(), q[i].Y()), rho0 = std::hypot(q[0].X(), q[0].Y());
            const double turn = sin_angle * (track.angle[i] - track.angle[0]);
            spread_angle = std::max(spread_angle, std::abs(turn) * rho);
            spread_along = std::max(spread_along, std::abs(rho - rho0));
          }
        }
        // The edge's ends at its vertices, where the edges of a loop meet
        // exactly (an edge's curve may end up to its tolerance from them).
        TopoDS_Vertex first_vertex, last_vertex;
        TopExp::Vertices(track.edge, first_vertex, last_vertex);
        const auto at_vertex = [&](const TopoDS_Vertex& v, std::size_t i) {
          const gp_Pnt p = v.IsNull() ? track.points[i] : BRep_Tool::Pnt(v);
          return std::make_pair(p, track.angle[i] + wrapped(raw_angle(p) - track.raw[i]) + turned);
        };
        const auto [start_point, start_angle] = at_vertex(first_vertex, 0);
        const auto [end_point, end_angle] = at_vertex(last_vertex, kSamples);
        const gp_Pnt2d start = chart(start_point, start_angle), end = chart(end_point, end_angle);
        if (spread_angle <= kOnLine || (!support.cone && spread_along <= kOnLine)) {
          // A generator, or a circle round a cylinder's axis: a line.
          edges.push_back(chart_line(start, end));
          continue;
        }
        if (spread_along <= kOnLine) {
          // A circle round a cone's axis: an arc about the apex.
          const double rho = (std::hypot(start.X(), start.Y()) + std::hypot(end.X(), end.Y())) / 2;
          detail::require_positive("a developed circle's radius", rho);
          const double a = sin_angle * start_angle;
          const double b = sin_angle * end_angle;
          const occ::handle<Geom2d_Circle> circle = new Geom2d_Circle(gp_Ax2d(gp_Pnt2d(0, 0), gp_Dir2d(1, 0)), rho);
          edges.push_back(detail::export_sheet_curve(circle, {std::min(a, b), std::max(a, b)}));
          continue;
        }
        const BRepAdaptor_Curve curve(track.edge);
        const auto exact = [&](double t) {
          const double at = (t - track.first) / (track.last - track.first) * kSamples;
          const std::size_t i = static_cast<std::size_t>(std::clamp(std::round(at), 0.0, double(kSamples)));
          const gp_Pnt p = curve.Value(t);
          const double angle = track.angle[i] + wrapped(raw_angle(p) - track.raw[i]);
          return chart(p, angle + turned);
        };
        // Each span where the edge's curve is twice continuous on its own
        // (a spline's knots of lower continuity change its speed, which no
        // cubic through evenly spaced points follows).
        const int spans = curve.NbIntervals(GeomAbs_C2);
        NCollection_Array1<double> bounds(1, spans + 1);
        curve.Intervals(bounds, GeomAbs_C2);
        for (int k = 1; k <= spans; ++k) {
          const double a = std::max(track.first, bounds(k)), b = std::min(track.last, bounds(k + 1));
          if (!(b > a)) continue;
          const occ::handle<Geom2d_BSplineCurve> developed = fitted(exact, a, b);
          if (a == track.first) developed->SetPole(1, start);
          if (b == track.last) developed->SetPole(developed->NbPoles(), end);
          edges.push_back(detail::export_sheet_curve(developed, {developed->FirstParameter(), developed->LastParameter()}));
        }
      }
      result.faces.push_back(std::move(edges));
    }
    return result;
  });
}

FaceBending face_bending(const Shape& shape, const std::string& name) {
  return detail::run("face bending", [&] {
    const std::vector<int> found = shape.find_faces(name);
    if (found.empty()) throw std::invalid_argument("the body has no such face");
    FaceBending result;
    Bnd_Box box;
    constexpr int kGrid = 12;
    for (const int f : found) {
      throw_if_cancelled();
      const TopoDS_Face face = shape.face(f);
      BRepBndLib::Add(face, box);
      const BRepAdaptor_Surface surface(face);
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      const double tolerance = BRep_Tool::Tolerance(face);
      // Where the face is (a trimmed face's points inside its loops); a
      // face too narrow for the grid takes every point of its bounds.
      double inside = 0, anywhere = 0;
      bool any_inside = false;
      for (int i = 0; i < kGrid; ++i) {
        for (int j = 0; j < kGrid; ++j) {
          const double u = u0 + (u1 - u0) * (i + 0.5) / kGrid;
          const double v = v0 + (v1 - v0) * (j + 0.5) / kGrid;
          BRepLProp_SLProps props(surface, u, v, 2, 1.0e-9);
          if (!props.IsCurvatureDefined()) continue;
          const double k = std::abs(props.GaussianCurvature());
          anywhere = std::max(anywhere, k);
          BRepClass_FaceClassifier classifier;
          classifier.Perform(face, gp_Pnt2d(u, v), tolerance);
          if (classifier.State() == TopAbs_IN) {
            inside = std::max(inside, k);
            any_inside = true;
          }
        }
      }
      result.gaussian = std::max(result.gaussian, any_inside ? inside : anywhere);
    }
    if (!box.IsVoid()) result.size = std::sqrt(box.SquareExtent());
    return result;
  });
}

} // namespace mitcad::geometry
