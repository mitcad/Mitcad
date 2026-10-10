// SPDX-License-Identifier: MIT
#include "bridge/datum.hpp"

#include <array>
#include <stdexcept>
#include <cstddef>
#include <string>
#include <utility>
#include <vector>

#include <gp.hxx>
#include <gp_Vec.hxx>

#include "bridge/analysis.hpp"
#include "mitcad/geometry/datum.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad_bridge/kernel/datum.h"

namespace mitcad::bridge {
namespace {

std::array<double, 3> xyz(const gp_XYZ& p) { return {p.X(), p.Y(), p.Z()}; }
std::array<double, 3> xyz(const gp_Pnt& p) { return xyz(p.XYZ()); }
std::array<double, 3> xyz(const gp_Dir& d) { return xyz(d.XYZ()); }

gp_Pnt point(const DatumVec& v) { return gp_Pnt(v.xyz[0], v.xyz[1], v.xyz[2]); }

gp_Dir direction(const DatumVec& v) {
  const gp_Vec vector(v.xyz[0], v.xyz[1], v.xyz[2]);
  if (vector.Magnitude() <= gp::Resolution()) {
    throw std::invalid_argument("a datum direction is zero");
  }
  return gp_Dir(vector);
}

DatumSurface surface(const geometry::SurfaceDescription& s) {
  DatumSurface result;
  result.kind = rust::String(s.type);
  result.origin = xyz(s.origin);
  result.axis = xyz(s.axis);
  result.radius = s.radius;
  result.minor_radius = s.minor_radius;
  result.half_angle = s.half_angle;
  return result;
}

DatumCurve curve(const geometry::CurveDescription& c) {
  DatumCurve result;
  result.kind = rust::String(c.type);
  result.start = xyz(c.start);
  result.end = xyz(c.end);
  result.center = xyz(c.center);
  result.normal = xyz(c.normal);
  result.radius = c.radius;
  return result;
}

} // namespace

DatumSurface datum_face_geometry(const geometry::Shape& shape, rust::Str face) {
  return surface(geometry::face_geometry(shape, std::string(face)));
}

DatumCurve datum_edge_geometry(const geometry::Shape& shape, rust::Str edge) {
  return curve(geometry::edge_geometry(shape, std::string(edge)));
}

DatumVec datum_vertex_point(const geometry::Shape& shape, rust::Str vertex) {
  DatumVec result;
  result.xyz = xyz(geometry::vertex_point(shape, std::string(vertex)));
  return result;
}

DatumPointing datum_path_point(const geometry::Shape& shape, rust::Slice<const rust::String> edges,
                               std::uint8_t mode, double value, const DatumVec& near) {
  std::vector<std::string> names;
  for (const rust::String& edge : edges) {
    names.emplace_back(edge);
  }
  const geometry::PathAt at = mode == 0   ? geometry::PathAt::Fraction
                              : mode == 1 ? geometry::PathAt::Length
                                          : geometry::PathAt::Near;
  const geometry::PathPoint p = geometry::path_point(shape, names, at, value, point(near));
  DatumPointing result;
  result.point = xyz(p.point);
  result.direction = xyz(p.tangent);
  return result;
}

DatumPointing datum_face_point(const geometry::Shape& shape, rust::Str face, const DatumVec& near) {
  // Where the point projects inside planes, cylinders or spheres, their
  // kept projections and classifiers give it (the import asks about the
  // edges of large faces again and again, mitcad#60: the distance query
  // sets up a classifier of the whole face every time).
  {
    const geometry::CheckedOperation check("face point");
    DatumPointing fast;
    if (analysis_face_point(shape, shape.find_faces(std::string(face)), near.xyz, fast.point,
                            fast.direction)) {
      return fast;
    }
  }
  const geometry::SurfacePoint p =
      geometry::face_point_normal(shape, std::string(face), point(near));
  DatumPointing result;
  result.point = xyz(p.point);
  result.direction = xyz(p.normal);
  return result;
}

std::shared_ptr<geometry::Shape> datum_plane_shape(const DatumVec& origin, const DatumVec& normal,
                                                   const DatumVec& x_axis, double size) {
  return geometry::plane_shape(point(origin), direction(normal), direction(x_axis), size);
}

std::shared_ptr<geometry::Shape> datum_axis_shape(const DatumVec& origin,
                                                  const DatumVec& direction_vec, double size) {
  return geometry::axis_shape(point(origin), direction(direction_vec), size);
}

std::shared_ptr<geometry::Shape> datum_point_shape(const DatumVec& p) {
  return geometry::point_shape(point(p));
}

rust::Vec<DatumFaceEntry> datum_face_geometries(const geometry::Shape& shape) {
  const std::vector<geometry::SurfaceDescription> surfaces = geometry::face_geometries(shape);
  rust::Vec<DatumFaceEntry> result;
  result.reserve(surfaces.size());
  for (std::size_t i = 0; i < surfaces.size(); ++i) {
    DatumFaceEntry entry;
    for (const std::string& name : shape.face_names(static_cast<int>(i))) {
      entry.names.push_back(rust::String(name));
    }
    entry.surface = surface(surfaces[i]);
    result.push_back(std::move(entry));
  }
  return result;
}

rust::Vec<DatumEdgeEntry> datum_edge_geometries(const geometry::Shape& shape) {
  const std::vector<geometry::CurveDescription> curves = geometry::edge_geometries(shape);
  rust::Vec<DatumEdgeEntry> result;
  result.reserve(curves.size());
  for (std::size_t i = 0; i < curves.size(); ++i) {
    DatumEdgeEntry entry;
    entry.name = rust::String(shape.edge_name(static_cast<int>(i)));
    entry.curve = curve(curves[i]);
    result.push_back(std::move(entry));
  }
  return result;
}

} // namespace mitcad::bridge
