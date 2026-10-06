// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sketch_ref.hpp"

#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRep_Tool.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_ApproxCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

occ::handle<Geom_BSplineCurve> as_bspline(const BRepAdaptor_Curve& curve) {
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  occ::handle<Geom_BSplineCurve> spline;
  if (curve.GetType() == GeomAbs_BSplineCurve) {
    spline = occ::handle<Geom_BSplineCurve>::DownCast(curve.BSpline()->Copy());
  } else if (curve.GetType() == GeomAbs_BezierCurve) {
    spline = GeomConvert::CurveToBSplineCurve(curve.Bezier());
  } else {
    GeomConvert_ApproxCurve approx(curve.Trim(first, last, 1e-9), 1e-6, GeomAbs_C2, 64, 9);
    if (!approx.HasResult()) {
      throw std::runtime_error("an edge curve could not be converted");
    }
    return approx.Curve();
  }
  if (spline->IsPeriodic()) {
    spline->SetNotPeriodic();
  }
  if (first > spline->FirstParameter() + 1e-12 || last < spline->LastParameter() - 1e-12) {
    spline->Segment(first, last);
  }
  return spline;
}

ModelCurve curve_of(const TopoDS_Edge& edge) {
  ModelCurve out;
  BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  switch (curve.GetType()) {
  case GeomAbs_Line:
    out.kind = ModelCurveKind::Line;
    out.start = curve.Value(first);
    out.end = curve.Value(last);
    return out;
  case GeomAbs_Circle:
  case GeomAbs_Ellipse: {
    out.kind = ModelCurveKind::Conic;
    gp_Ax2 position;
    if (curve.GetType() == GeomAbs_Circle) {
      const gp_Circ circle = curve.Circle();
      position = circle.Position();
      out.major = out.minor = circle.Radius();
    } else {
      const gp_Elips ellipse = curve.Ellipse();
      position = ellipse.Position();
      out.major = ellipse.MajorRadius();
      out.minor = ellipse.MinorRadius();
    }
    out.center = position.Location();
    out.normal = position.Direction();
    out.x_axis = position.XDirection();
    out.first = first;
    out.last = last;
    out.closed = last - first >= kTwoPi - 1e-9;
    return out;
  }
  default: {
    out.kind = ModelCurveKind::BSpline;
    const occ::handle<Geom_BSplineCurve> spline = as_bspline(curve);
    out.degree = spline->Degree();
    for (int i = 1; i <= spline->NbPoles(); ++i) {
      out.poles.push_back(spline->Pole(i));
    }
    if (spline->IsRational()) {
      for (int i = 1; i <= spline->NbPoles(); ++i) {
        out.weights.push_back(spline->Weight(i));
      }
    }
    const auto& knots = spline->KnotSequence();
    for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
      out.knots.push_back(knots(i));
    }
    return out;
  }
  }
}

bool starts_with(const std::string& text, const char* prefix) { return text.rfind(prefix, 0) == 0; }

// The non-degenerate boundary edges of face f, each once.
std::vector<int> face_edges(const Shape& shape, int f) {
  std::vector<int> edges;
  for (TopExp_Explorer it(shape.face(f), TopAbs_EDGE); it.More(); it.Next()) {
    const int e = shape.edge_index(it.Current());
    bool seen = false;
    for (const int known : edges) {
      seen = seen || known == e;
    }
    if (e >= 0 && !seen && !BRep_Tool::Degenerated(TopoDS::Edge(it.Current()))) {
      edges.push_back(e);
    }
  }
  return edges;
}

} // namespace

std::vector<ModelCurve> curves_of(const Shape& shape, const std::string& name) {
  return detail::run("model curves", [&] {
    std::vector<ModelCurve> curves;
    if (starts_with(name, "V{")) {
      for (const int i : shape.find_vertices(name)) {
        ModelCurve point;
        point.kind = ModelCurveKind::Point;
        point.start = BRep_Tool::Pnt(shape.vertex(i));
        curves.push_back(point);
      }
      return curves;
    }
    std::vector<int> edges;
    if (starts_with(name, "E{")) {
      edges = shape.find_edges(name);
    } else {
      for (const int f : shape.find_faces(name)) {
        for (const int e : face_edges(shape, f)) {
          bool seen = false;
          for (const int known : edges) {
            seen = seen || known == e;
          }
          if (!seen) {
            edges.push_back(e);
          }
        }
      }
    }
    for (const int e : edges) {
      curves.push_back(curve_of(shape.edge(e)));
    }
    return curves;
  });
}

IndexedElement indexed_element(const Shape& shape, char kind, int index) {
  return detail::run("element by index", [&] {
    IndexedElement out;
    if (index < 0) {
      return out;
    }
    switch (kind) {
    case 'F':
      if (index < shape.face_count()) {
        out.found = true;
        const NameList& names = shape.face_names(index);
        if (!names.empty()) {
          out.name = names.front();
        }
        for (const int e : face_edges(shape, index)) {
          out.curves.push_back(curve_of(shape.edge(e)));
        }
      }
      break;
    case 'E':
      if (index < shape.edge_count()) {
        out.found = true;
        out.name = shape.edge_name(index);
        if (!BRep_Tool::Degenerated(shape.edge(index))) {
          out.curves.push_back(curve_of(shape.edge(index)));
        }
      }
      break;
    case 'V':
      if (index < shape.vertex_count()) {
        out.found = true;
        out.name = shape.vertex_name(index);
        ModelCurve point;
        point.kind = ModelCurveKind::Point;
        point.start = BRep_Tool::Pnt(shape.vertex(index));
        out.curves.push_back(point);
      }
      break;
    default:
      throw std::invalid_argument(std::string("an element kind F, E or V, not ") + kind);
    }
    return out;
  });
}

} // namespace mitcad::geometry
