// SPDX-License-Identifier: MIT
#include "bridge/sheet.hpp"

#include <array>
#include <cmath>
#include <stdexcept>
#include <string>

#include "mitcad/geometry/sheet_footprint.hpp"
#include "mitcad/geometry/sheet_trim.hpp"
#include "mitcad_bridge/kernel/sheet.h"

namespace mitcad::bridge {
namespace {
namespace g = geometry;

void unit_axis(const std::array<double, 3>& axis) {
  double squared = 0;
  for (double value : axis) {
    if (!std::isfinite(value)) throw std::invalid_argument("sheet frame axes must be finite unit vectors");
    squared += value * value;
  }
  if (std::abs(squared - 1) > 1.0e-9) throw std::invalid_argument("sheet frame axes must be unit vectors");
}

g::SheetDomainRef input(const SheetDomain& d) {
  if (d.kind > 1) throw std::invalid_argument("unknown sheet domain kind");
  return {d.kind == 0 ? g::SheetDomainKind::Panel : g::SheetDomainKind::Bend, std::string(d.feature), d.slot};
}
SheetDomain output(const g::SheetDomainRef& d) {
  return {static_cast<std::uint8_t>(d.kind == g::SheetDomainKind::Panel ? 0 : 1), rust::String(d.feature), d.slot};
}
g::ParamSpan input(const SheetSpan& s) { return {s.first, s.last}; }
SheetSpan output(g::ParamSpan s) { return {s.first, s.last}; }
g::CurvePieceRef input(const SheetPiece& p) { return {std::string(p.curve), p.piece}; }
SheetPiece output(const g::CurvePieceRef& p) { return {rust::String(p.curve), p.piece}; }
g::BoundaryPieceRef input(const SheetBoundary& b) { return {input(b.domain), input(b.piece)}; }
g::AxialMap input(const SheetAxial& a) { return {a.offset, a.scale}; }
g::ShapePortSide input(const SheetPortSide& p) { return {input(p.boundary), input(p.span), input(p.axial)}; }
g::ShapePort input(const SheetPort& p) { return {p.token, input(p.axial_interval), input(p.first), input(p.second)}; }
g::MaterialRegion input(const SheetRegion& r) {
  g::MaterialRegion result;
  for (const auto& l : r.loops) {
    g::MaterialLoop loop;
    for (const auto& e : l.edges) loop.edges.push_back({input(e.piece), {to_geometry(e.curve.geometry), input(e.curve.range)}});
    result.loops.push_back(std::move(loop));
  }
  return result;
}
template <class Regions>
std::vector<g::MaterialRegion> regions(const Regions& values) {
  std::vector<g::MaterialRegion> result;
  for (const auto& r : values) result.push_back(input(r));
  return result;
}
SheetCurve output(const g::BoundedCurve2& c) { return {from_geometry(c.geometry), output(c.range)}; }
SheetEndpoint output(const g::TrimEndpoint& e) { return {output(e.domain), e.edge_token, e.parameter}; }
SheetEndpointSpan output(const g::TrimEndpointSpan& e) { return {output(e.domain), e.edge_token, output(e.range)}; }

} // namespace

std::shared_ptr<geometry::Shape> SheetShapeResult::solid(std::size_t index) const { return m_result.solids.at(index).shape; }

rust::Vec<SheetMember> SheetShapeResult::membership(std::size_t index) const {
  rust::Vec<SheetMember> result;
  for (const auto& m : m_result.solids.at(index).membership) result.push_back({output(m.domain), m.region});
  return result;
}

rust::Vec<rust::String> SheetShapeResult::diagnostics() const {
  rust::Vec<rust::String> result;
  for (const auto& d : m_result.diagnostics) result.push_back(rust::String(d));
  return result;
}

std::unique_ptr<SheetShapeResult> sheet_shape(rust::Slice<const SheetPatchInput> patches,
                                             rust::Slice<const SheetPort> ports, double thickness, double tolerance) {
  geometry::SheetChartShapeSpec spec;
  spec.thickness = thickness; spec.tolerance = tolerance;
  for (const auto& p : patches) {
    unit_axis(p.x_axis); unit_axis(p.y_axis);
    geometry::SheetPatch patch{input(p.domain), regions(p.regions), {}};
    if (p.cylinder) {
      unit_axis(p.axis);
      patch.embedding = geometry::CylinderEmbedding{
          {p.origin[0], p.origin[1], p.origin[2]}, {p.axis[0], p.axis[1], p.axis[2]},
          {p.x_axis[0], p.x_axis[1], p.x_axis[2]}, {p.y_axis[0], p.y_axis[1], p.y_axis[2]},
          p.inside_radius, p.neutral_radius, p.angle_extent, p.inner_is_front};
    } else {
      if (p.parity != 1 && p.parity != -1) throw std::invalid_argument("unknown sheet material parity");
      patch.embedding = geometry::PlaneEmbedding{to_geometry(Frame{p.origin, p.x_axis, p.y_axis}),
          p.parity == 1 ? geometry::MaterialParity::Positive : geometry::MaterialParity::Negative};
    }
    spec.patches.push_back(std::move(patch));
  }
  for (const auto& p : ports) spec.ports.push_back(input(p));
  return std::make_unique<SheetShapeResult>(geometry::sheet_shape(spec));
}

SheetTrimOutput sheet_trim(rust::Slice<const SheetTrimDomain> domains, rust::Slice<const SheetRegion> tools,
                          rust::Slice<const SheetPort> seams, std::uint8_t operation, double tolerance) {
  geometry::SheetTrimSpec spec;
  switch (operation) {
  case 0: spec.operation = geometry::SheetTrimOperation::Union; break;
  case 1: spec.operation = geometry::SheetTrimOperation::Difference; break;
  case 2: spec.operation = geometry::SheetTrimOperation::Intersection; break;
  default: throw std::invalid_argument("unknown sheet trim operation");
  }
  spec.tolerance = tolerance; spec.tools = regions(tools);
  for (const auto& d : domains) spec.domains.push_back({input(d.domain), regions(d.regions),
      {{d.origin[0], d.origin[1]}, {d.x_axis[0], d.x_axis[1]}, {d.y_axis[0], d.y_axis[1]}}});
  for (const auto& s : seams) {
    const auto p = input(s);
    spec.seams.push_back({p.token, {p.first.boundary, p.first.span, p.first.axial},
                         {p.second.boundary, p.second.span, p.second.axial}, p.axial_interval});
  }
  const auto trimmed = geometry::sheet_trim_domain(spec);
  SheetTrimOutput result;
  for (const auto& d : trimmed.domains) {
    SheetTrimDomainOutput domain{output(d.domain), {}};
    for (const auto& r : d.regions) {
      SheetTrimRegion region{r.token, {}};
      for (const auto& l : r.loops) {
        SheetTrimLoop loop;
        for (const auto& e : l.edges) {
          SheetTrimEdge edge{e.token, output(e.curve), {}};
          for (const auto& lineage : e.lineage) edge.lineage.push_back({lineage.source.kind == geometry::TrimSourceKind::Tool,
              {output(lineage.source.domain), output(lineage.source.piece)}, output(lineage.span)});
          loop.edges.push_back(std::move(edge));
        }
        region.loops.push_back(std::move(loop));
      }
      domain.regions.push_back(std::move(region));
    }
    result.domains.push_back(std::move(domain));
  }
  for (const auto& s : trimmed.splits) {
    std::uint8_t kind = 2;
    switch (s.kind) {
    case geometry::SeamEventKind::Crossing: kind = 0; break;
    case geometry::SeamEventKind::Tangency: kind = 1; break;
    case geometry::SeamEventKind::Endpoint: kind = 2; break;
    }
    SheetSplit split{s.seam_token, s.split_token, s.axial, kind, {}, {}};
    for (const auto& e : s.first) split.first.push_back(output(e));
    for (const auto& e : s.second) split.second.push_back(output(e));
    result.splits.push_back(std::move(split));
  }
  for (const auto& p : trimmed.ports) result.ports.push_back({p.seam_token, output(p.axial_interval), output(p.first), output(p.second)});
  for (const auto& d : trimmed.diagnostics) result.diagnostics.push_back(rust::String(d));
  return result;
}

rust::Vec<SheetFootprintFace> sheet_cylinder_footprint(const geometry::Shape& shape,
                                                      const SheetFootprintCylinder& cylinder, double tolerance) {
  unit_axis(cylinder.axis); unit_axis(cylinder.radial_start); unit_axis(cylinder.angular_direction);
  geometry::CylinderEmbedding chart;
  chart.centre = gp_Pnt(cylinder.centre[0], cylinder.centre[1], cylinder.centre[2]);
  chart.axis = gp_Dir(cylinder.axis[0], cylinder.axis[1], cylinder.axis[2]);
  chart.radial_start = gp_Dir(cylinder.radial_start[0], cylinder.radial_start[1], cylinder.radial_start[2]);
  chart.angular_direction = gp_Dir(cylinder.angular_direction[0], cylinder.angular_direction[1],
                                   cylinder.angular_direction[2]);
  chart.neutral_radius = cylinder.neutral_radius;
  chart.angle_extent = cylinder.angle_extent;
  rust::Vec<SheetFootprintFace> result;
  for (const auto& face : geometry::sheet_cylinder_footprint(shape, chart, cylinder.radius, tolerance)) {
    SheetFootprintFace out;
    for (const auto& edge : face) out.edges.push_back(output(edge));
    result.push_back(std::move(out));
  }
  return result;
}

} // namespace mitcad::bridge
