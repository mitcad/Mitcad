// SPDX-License-Identifier: MIT
#include "bridge/drawing.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <utility>
#include <vector>

#include "mitcad/geometry/drawing.hpp"
#include "mitcad_bridge/kernel/drawing.h"

namespace mitcad::bridge {
namespace {

gp_Pnt point(const std::array<double, 3>& p) { return gp_Pnt(p[0], p[1], p[2]); }
gp_Dir direction(const std::array<double, 3>& d) { return gp_Dir(d[0], d[1], d[2]); }

void push_points(const std::vector<geometry::Point2>& points, rust::Vec<double>& out) {
  for (const geometry::Point2& p : points) {
    out.push_back(p[0]);
    out.push_back(p[1]);
  }
}

} // namespace

DrawingProjection project_view(const ShapeList& shapes, const DrawingView& view) {
  geometry::DrawingViewSpec spec;
  spec.toward_eye = direction(view.toward_eye);
  spec.x_axis = direction(view.x_axis);
  spec.hidden = view.hidden;
  spec.smooth = view.smooth;
  spec.exact = view.exact;
  spec.quick = view.quick;
  if (view.has_section) {
    spec.section = geometry::DrawingSection{point(view.section_origin), direction(view.section_normal)};
  }
  spec.deflection = view.deflection;
  const geometry::DrawnView drawn = geometry::project_view(shapes.items(), spec);
  DrawingProjection out;
  out.polygonal = drawn.polygonal;
  for (const geometry::DrawnCurve& c : drawn.curves) {
    DrawingCurve curve;
    curve.visible = c.visible;
    curve.kind = static_cast<std::uint8_t>(c.kind == geometry::DrawnKind::Sharp    ? 0
                                           : c.kind == geometry::DrawnKind::Smooth ? 1
                                                                                   : 2);
    curve.shape = static_cast<std::uint8_t>(c.type == geometry::DrawnCurve::Type::Line       ? 0
                                            : c.type == geometry::DrawnCurve::Type::Arc      ? 1
                                            : c.type == geometry::DrawnCurve::Type::Polyline ? 2
                                                                                             : 3);
    push_points(c.points, curve.points);
    curve.center = {c.center[0], c.center[1]};
    curve.radius = c.radius;
    curve.start = c.start;
    curve.end = c.end;
    // A B-spline's own data (mitcad#853).
    curve.degree = static_cast<std::uint32_t>(c.degree);
    for (const double knot : c.knots) {
      curve.knots.push_back(knot);
    }
    push_points(c.poles, curve.poles);
    for (const double weight : c.weights) {
      curve.weights.push_back(weight);
    }
    out.curves.push_back(std::move(curve));
  }
  for (std::size_t face = 0; face < drawn.cuts.size(); ++face) {
    for (const std::vector<geometry::Point2>& loop : drawn.cuts[face]) {
      DrawingLoop cut;
      cut.face = face;
      push_points(loop, cut.points);
      out.cuts.push_back(std::move(cut));
    }
  }
  return out;
}

rust::Vec<DrawingHole> round_holes(const geometry::Shape& shape) {
  rust::Vec<DrawingHole> out;
  for (const geometry::RoundHole& h : geometry::round_holes(shape)) {
    DrawingHole hole;
    hole.face = rust::String(h.face);
    hole.opening = {h.opening.X(), h.opening.Y(), h.opening.Z()};
    hole.direction = {h.direction.X(), h.direction.Y(), h.direction.Z()};
    hole.diameter = h.diameter;
    hole.through = h.through;
    hole.depth = h.depth;
    hole.cbore_diameter = h.cbore_diameter;
    hole.cbore_depth = h.cbore_depth;
    hole.csink_diameter = h.csink_diameter;
    hole.csink_angle = h.csink_angle;
    out.push_back(std::move(hole));
  }
  return out;
}

} // namespace mitcad::bridge
