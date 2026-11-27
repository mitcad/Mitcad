// SPDX-License-Identifier: MIT
#include "mitcad/geometry/iso_curve.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Geom2dAPI_InterCurveCurve.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <TopAbs_State.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/user_error.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// A tolerance in a face's parameters: where a parameter line meets the
// face's boundary (as face_outline.cpp's silhouettes cut theirs).
constexpr double kParameter = 1e-9;
// The tolerance of the classification of a parameter line's pieces.
constexpr double kClassify = 1e-7;
// The shortest piece kept (mm).
constexpr double kShortest = 1e-7;

// A face's range of its surface's parameters.
struct Range {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
};

// The point of the face's parameter plane at `along` on the parameter line
// that holds `value`.
gp_Pnt2d on_line(bool hold_u, double value, double along) {
  return hold_u ? gp_Pnt2d(value, along) : gp_Pnt2d(along, value);
}

// The pieces of the face's parameter line that holds `value` (u with
// `hold_u`, else v) between `low` and `high` of the other parameter that
// lie on the face (inside it or on its boundary), as (from, to): the line
// is cut where it meets the curves of the face's edges on its surface, and
// each piece between cuts is classified by its middle.
std::vector<std::pair<double, double>> pieces_on_face(const TopoDS_Face& face, bool hold_u, double value, double low,
                                                      double high) {
  std::vector<double> cuts{low, high};
  const double reach = 1.0 + (high - low);
  const occ::handle<Geom2d_Line> line =
      new Geom2d_Line(on_line(hold_u, value, 0.0), hold_u ? gp_Dir2d(0.0, 1.0) : gp_Dir2d(1.0, 0.0));
  const occ::handle<Geom2d_Curve> along = new Geom2d_TrimmedCurve(line, low - reach, high + reach);
  const auto held = [hold_u](const gp_Pnt2d& p) { return hold_u ? p.X() : p.Y(); };
  const auto other = [hold_u](const gp_Pnt2d& p) { return hold_u ? p.Y() : p.X(); };
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull() || !(last > first)) {
      continue;
    }
    // An edge's curve along the line (a seam, a straight side) meets it all
    // along: its ends cut it.
    for (const double t : {first, last}) {
      const gp_Pnt2d end = pcurve->Value(t);
      if (std::abs(held(end) - value) <= kParameter) {
        cuts.push_back(other(end));
      }
    }
    const occ::handle<Geom2d_Curve> piece = new Geom2d_TrimmedCurve(pcurve, first, last);
    const Geom2dAPI_InterCurveCurve crossing(piece, along, kParameter);
    for (int i = 1; i <= crossing.NbPoints(); ++i) {
      cuts.push_back(other(crossing.Point(i)));
    }
    for (int i = 1; i <= crossing.NbSegments(); ++i) {
      occ::handle<Geom2d_Curve> on_edge;
      occ::handle<Geom2d_Curve> on_line_part;
      crossing.Segment(i, on_edge, on_line_part);
      if (!on_line_part.IsNull()) {
        cuts.push_back(other(on_line_part->Value(on_line_part->FirstParameter())));
        cuts.push_back(other(on_line_part->Value(on_line_part->LastParameter())));
      }
    }
  }
  for (double& t : cuts) {
    t = std::clamp(t, low, high);
  }
  std::sort(cuts.begin(), cuts.end());
  std::vector<double> stops;
  for (const double t : cuts) {
    if (stops.empty() || t - stops.back() > kParameter) {
      stops.push_back(t);
    }
  }
  std::vector<std::pair<double, double>> pieces;
  for (std::size_t i = 0; i + 1 < stops.size(); ++i) {
    BRepClass_FaceClassifier classifier(face, on_line(hold_u, value, 0.5 * (stops[i] + stops[i + 1])), kClassify);
    const TopAbs_State state = classifier.State();
    if (state != TopAbs_IN && state != TopAbs_ON) {
      continue;
    }
    if (!pieces.empty() && pieces.back().second >= stops[i] - kParameter) {
      pieces.back().second = stops[i + 1];
    } else {
      pieces.emplace_back(stops[i], stops[i + 1]);
    }
  }
  return pieces;
}

// The held parameter's value where the curve passes through the face's
// point nearest `point`, brought into the face's range on a periodic
// surface.
double held_through(const std::vector<TopoDS_Face>& faces, bool hold_u, const gp_Pnt& point, const Range& range) {
  double best = std::numeric_limits<double>::infinity();
  double value = 0.0;
  for (const TopoDS_Face& face : faces) {
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), face);
    if (!distance.IsDone() || distance.NbSolution() < 1 || !(distance.Value() < best)) {
      continue;
    }
    best = distance.Value();
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
    ShapeAnalysis_Surface analysis(surface);
    const gp_Pnt2d uv = analysis.ValueOfUV(distance.PointOnShape2(1), 1e-7);
    value = hold_u ? uv.X() : uv.Y();
    const bool periodic = hold_u ? surface->IsUPeriodic() : surface->IsVPeriodic();
    const double low = hold_u ? range.u0 : range.v0;
    const double high = hold_u ? range.u1 : range.v1;
    if (periodic) {
      const double period = hold_u ? surface->UPeriod() : surface->VPeriod();
      if (period > 0.0) {
        while (value < low - kParameter) {
          value += period;
        }
        while (value > high + kParameter) {
          value -= period;
        }
      }
    }
    value = std::clamp(value, low, high);
  }
  if (!std::isfinite(best)) {
    throw std::runtime_error("the face's point nearest to the picked point could not be found");
  }
  return value;
}

// The fraction as the error's detail gives it.
std::string fraction_text(double fraction) {
  char text[32];
  std::snprintf(text, sizeof(text), "%.6g", fraction);
  return text;
}

} // namespace

std::vector<IsoCurve> iso_curves(const Shape& shape, const std::string& face, IsoParameter held,
                                 const std::vector<double>& fractions, const std::optional<gp_Pnt>& through) {
  const bool hold_u = held == IsoParameter::U;
  if (!through) {
    if (fractions.empty()) {
      throw std::invalid_argument("an isoparametric curve needs a place on the face");
    }
    for (const double fraction : fractions) {
      if (!(fraction >= 0.0 && fraction <= 1.0)) {
        throw std::invalid_argument("an isoparametric curve's position must lie from 0 to 1 across the face, got " +
                                    fraction_text(fraction));
      }
    }
  }
  return detail::run("isoparametric curve", [&] {
    const std::vector<int> found = shape.find_faces(face);
    if (found.empty()) {
      throw std::invalid_argument(no_face_error(face));
    }
    std::vector<TopoDS_Face> faces;
    Range range{std::numeric_limits<double>::infinity(), -std::numeric_limits<double>::infinity(),
                std::numeric_limits<double>::infinity(), -std::numeric_limits<double>::infinity()};
    for (const int f : found) {
      const TopoDS_Face& piece = shape.face(f);
      double u0 = 0.0;
      double u1 = 0.0;
      double v0 = 0.0;
      double v1 = 0.0;
      BRepTools::UVBounds(piece, u0, u1, v0, v1);
      range.u0 = std::min(range.u0, u0);
      range.u1 = std::max(range.u1, u1);
      range.v0 = std::min(range.v0, v0);
      range.v1 = std::max(range.v1, v1);
      faces.push_back(piece);
    }
    const double low = hold_u ? range.u0 : range.v0;
    const double high = hold_u ? range.u1 : range.v1;
    const double span = high - low;
    if (!(span > kParameter) || !std::isfinite(span)) {
      throw std::invalid_argument(face_error(face, "the face does not reach across that parameter",
                                             "face " + face + " has no range of its " + (hold_u ? "u" : "v") +
                                                 " parameter"));
    }
    // The held values and where they lie in the range.
    std::vector<std::pair<double, double>> places;
    if (through) {
      const double value = held_through(faces, hold_u, *through, range);
      places.emplace_back(value, (value - low) / span);
    } else {
      for (const double fraction : fractions) {
        places.emplace_back(low + fraction * span, fraction);
      }
    }
    std::vector<IsoCurve> out;
    for (const auto& [value, fraction] : places) {
      IsoCurve curve;
      curve.fraction = fraction;
      curve.parameter = value;
      for (const TopoDS_Face& piece : faces) {
        const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(piece);
        double u0 = 0.0;
        double u1 = 0.0;
        double v0 = 0.0;
        double v1 = 0.0;
        BRepTools::UVBounds(piece, u0, u1, v0, v1);
        const double from = hold_u ? v0 : u0;
        const double to = hold_u ? v1 : u1;
        const occ::handle<Geom_Curve> iso = hold_u ? surface->UIso(value) : surface->VIso(value);
        if (iso.IsNull()) {
          continue;
        }
        for (auto [a, b] : pieces_on_face(piece, hold_u, value, from, to)) {
          if (!iso->IsPeriodic()) {
            a = std::max(a, iso->FirstParameter());
            b = std::min(b, iso->LastParameter());
          }
          if (!(b - a > kParameter)) {
            continue;
          }
          BRepBuilderAPI_MakeEdge make(iso, a, b);
          if (!make.IsDone()) {
            throw std::runtime_error("an isoparametric curve could not be made");
          }
          const TopoDS_Edge edge = make.Edge();
          if (GCPnts_AbscissaPoint::Length(BRepAdaptor_Curve(edge)) <= kShortest) {
            continue;
          }
          curve.pieces.push_back(edge_curve(edge));
        }
      }
      if (curve.pieces.empty()) {
        throw std::invalid_argument(face_error(face, "the isoparametric curve there runs nowhere on the face",
                                               "the curve at " + fraction_text(fraction) + " of face " + face +
                                                   "'s range runs nowhere inside it"));
      }
      out.push_back(std::move(curve));
    }
    return out;
  });
}

} // namespace mitcad::geometry
