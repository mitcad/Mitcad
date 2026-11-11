// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sheet_footprint.hpp"

#include <cmath>
#include <stdexcept>

#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Cylinder.hxx>

#include "sheet_curve.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;

// The affine map from a cylindrical face's parameters (a, b) to the chart:
// u = scale_u * a + offset_u, v = scale_v * b + offset_v.
struct ChartMap {
  double scale_u = 1, offset_u = 0, scale_v = 1, offset_v = 0;
  gp_Pnt2d operator()(const gp_Pnt2d& p) const {
    return {scale_u * p.X() + offset_u, scale_v * p.Y() + offset_v};
  }
};

} // namespace

std::vector<std::vector<BoundedCurve2>> sheet_cylinder_footprint(const Shape& shape,
                                                                 const CylinderEmbedding& chart,
                                                                 double radius, double tolerance) {
  return detail::run("sheet cylinder footprint", [&] {
    detail::require_positive("sheet footprint radius", radius);
    detail::require_positive("sheet footprint neutral radius", chart.neutral_radius);
    const gp_Vec axis(chart.axis), start(chart.radial_start), across(chart.angular_direction);
    std::vector<std::vector<BoundedCurve2>> result;
    for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More(); f.Next()) {
      throw_if_cancelled();
      const TopoDS_Face face = TopoDS::Face(f.Current());
      const GeomAdaptor_Surface surface(BRep_Tool::Surface(face));
      if (surface.GetType() != GeomAbs_Cylinder) continue;
      const gp_Cylinder cylinder = surface.Cylinder();
      const gp_Ax3& position = cylinder.Position();
      const gp_Vec x(position.XDirection()), y(position.YDirection()), z(position.Direction());
      const gp_Vec offset(chart.centre, position.Location());
      if (std::abs(cylinder.Radius() - radius) > tolerance || std::abs(std::abs(z.Dot(axis)) - 1) > 1.0e-9 ||
          (offset - offset.Dot(axis) * axis).Magnitude() > tolerance) {
        continue;
      }
      // The face's angle a is the chart's angle alpha + sense * a, its
      // height b the chart's v0 + (z . axis) * b.
      const double alpha = std::atan2(x.Dot(across), x.Dot(start));
      const double sense = x.Crossed(y).Dot(start.Crossed(across)) > 0 ? 1.0 : -1.0;
      // One turn of the face's parameters: the one centred on the bend.
      double u_min = 0, u_max = 0, v_min = 0, v_max = 0;
      BRepTools::UVBounds(face, u_min, u_max, v_min, v_max);
      double middle = alpha + sense * (u_min + u_max) / 2;
      double turn = 0;
      while (middle + turn < chart.angle_extent / 2 - kPi) turn += 2 * kPi;
      while (middle + turn >= chart.angle_extent / 2 + kPi) turn -= 2 * kPi;
      const ChartMap map{chart.neutral_radius * sense, chart.neutral_radius * (alpha + turn), z.Dot(axis),
                         offset.Dot(axis)};
      std::vector<BoundedCurve2> edges;
      for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
        const TopoDS_Edge edge = TopoDS::Edge(e.Current());
        if (BRep_Tool::Degenerated(edge)) continue;
        double first = 0, last = 0;
        const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
        if (pcurve.IsNull()) throw std::runtime_error("a face's edge has no parameter curve on its cylinder");
        occ::handle<Geom2d_Curve> basis = pcurve;
        while (const auto trimmed = occ::handle<Geom2d_TrimmedCurve>::DownCast(basis)) basis = trimmed->BasisCurve();
        if (occ::handle<Geom2d_Line>::DownCast(basis)) {
          const gp_Pnt2d a = map(basis->Value(first)), b = map(basis->Value(last));
          const double length = a.Distance(b);
          if (length <= tolerance) continue;
          const occ::handle<Geom2d_Line> mapped = new Geom2d_Line(a, gp_Dir2d(gp_Vec2d(a, b)));
          edges.push_back(detail::export_sheet_curve(mapped, {0.0, length}));
          continue;
        }
        occ::handle<Geom2d_BSplineCurve> spline =
            Geom2dConvert::CurveToBSplineCurve(new Geom2d_TrimmedCurve(basis, first, last));
        if (spline.IsNull()) throw std::runtime_error("a face's parameter curve has no exact spline form");
        spline = occ::handle<Geom2d_BSplineCurve>::DownCast(spline->Copy());
        for (int i = 1; i <= spline->NbPoles(); ++i) spline->SetPole(i, map(spline->Pole(i)));
        edges.push_back(detail::export_sheet_curve(spline, {spline->FirstParameter(), spline->LastParameter()}));
      }
      result.push_back(std::move(edges));
    }
    return result;
  });
}

} // namespace mitcad::geometry
