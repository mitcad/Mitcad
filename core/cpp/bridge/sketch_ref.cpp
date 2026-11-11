// SPDX-License-Identifier: MIT
#include "bridge/sketch_ref.hpp"

#include <array>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <utility>

#include "mitcad/geometry/sketch_ref.hpp"
#include "mitcad_bridge/kernel/sketch_ref.h"

namespace mitcad::bridge {
namespace {

std::array<double, 3> xyz(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

std::array<double, 3> xyz(const gp_Dir& d) { return {d.X(), d.Y(), d.Z()}; }

ModelCurve convert(const geometry::ModelCurve& c) {
  ModelCurve out{};
  switch (c.kind) {
  case geometry::ModelCurveKind::Point:
    out.kind = ModelCurveKind::Point;
    break;
  case geometry::ModelCurveKind::Line:
    out.kind = ModelCurveKind::Line;
    break;
  case geometry::ModelCurveKind::Conic:
    out.kind = ModelCurveKind::Conic;
    break;
  case geometry::ModelCurveKind::BSpline:
    out.kind = ModelCurveKind::BSpline;
    break;
  case geometry::ModelCurveKind::Helix:
    // (Helices are paths only, mitcad#385: a shape's edges come as
    // B-splines.)
    throw std::invalid_argument("a helix is not an edge's curve");
  }
  out.start = xyz(c.start);
  out.end = xyz(c.end);
  out.center = xyz(c.center);
  out.normal = xyz(c.normal);
  out.x_axis = xyz(c.x_axis);
  out.major = c.major;
  out.minor = c.minor;
  out.first = c.first;
  out.last = c.last;
  out.closed = c.closed;
  out.degree = static_cast<std::uint32_t>(c.degree);
  for (const gp_Pnt& p : c.poles) {
    out.poles.push_back(p.X());
    out.poles.push_back(p.Y());
    out.poles.push_back(p.Z());
  }
  for (const double w : c.weights) {
    out.weights.push_back(w);
  }
  for (const double k : c.knots) {
    out.knots.push_back(k);
  }
  return out;
}

} // namespace

rust::Vec<ModelCurve> model_curves(const geometry::Shape& shape, rust::Str name) {
  rust::Vec<ModelCurve> result;
  for (const geometry::ModelCurve& c : geometry::curves_of(shape, std::string(name))) {
    result.push_back(convert(c));
  }
  return result;
}

rust::Vec<NamedBoundaryCurve> face_boundary(const geometry::Shape& shape, rust::Str face) {
  rust::Vec<NamedBoundaryCurve> result;
  for (const geometry::NamedBoundaryCurve& c : geometry::face_boundary(shape, std::string(face))) {
    NamedBoundaryCurve edge{};
    edge.name = rust::String(c.name);
    edge.curve = convert(c.curve);
    result.push_back(std::move(edge));
  }
  return result;
}

IndexedElement indexed_element(const geometry::Shape& shape, std::uint8_t kind, std::size_t index) {
  const geometry::IndexedElement element =
      geometry::indexed_element(shape, static_cast<char>(kind), static_cast<int>(index));
  IndexedElement out{};
  out.found = element.found;
  out.name = rust::String(element.name);
  for (const geometry::ModelCurve& c : element.curves) {
    out.curves.push_back(convert(c));
  }
  return out;
}

rust::Vec<NamedBoundaryCurve> named_edge_curves(const geometry::Shape& shape) {
  rust::Vec<NamedBoundaryCurve> result;
  for (const geometry::NamedBoundaryCurve& c : geometry::named_edge_curves(shape)) {
    NamedBoundaryCurve edge{};
    edge.name = rust::String(c.name);
    edge.curve = convert(c.curve);
    result.push_back(std::move(edge));
  }
  return result;
}

rust::Vec<SectionPiece> section_curves(const geometry::Shape& shape, rust::Str name, std::array<double, 3> origin,
                                       std::array<double, 3> normal) {
  rust::Vec<SectionPiece> result;
  for (const geometry::SectionCurve& c :
       geometry::section_curves(shape, std::string(name), gp_Pnt(origin[0], origin[1], origin[2]),
                                gp_Dir(normal[0], normal[1], normal[2]))) {
    SectionPiece piece{};
    piece.curve = convert(c.curve);
    for (const std::string& face : c.faces) {
      piece.faces.push_back(rust::String(face));
    }
    result.push_back(std::move(piece));
  }
  return result;
}

rust::Vec<double> section_points(const geometry::Shape& shape, rust::Str name, std::array<double, 3> origin,
                                 std::array<double, 3> normal) {
  rust::Vec<double> result;
  for (const gp_Pnt& p :
       geometry::section_points(shape, std::string(name), gp_Pnt(origin[0], origin[1], origin[2]),
                                gp_Dir(normal[0], normal[1], normal[2]))) {
    result.push_back(p.X());
    result.push_back(p.Y());
    result.push_back(p.Z());
  }
  return result;
}

} // namespace mitcad::bridge
