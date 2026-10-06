// SPDX-License-Identifier: MIT
#include "bridge/datum.hpp"

#include <array>
#include <stdexcept>
#include <string>
#include <vector>

#include <gp.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/datum.hpp"
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

} // namespace

DatumSurface datum_face_geometry(const geometry::Shape& shape, rust::Str face) {
  const geometry::SurfaceDescription s = geometry::face_geometry(shape, std::string(face));
  DatumSurface result;
  result.kind = rust::String(s.type);
  result.origin = xyz(s.origin);
  result.axis = xyz(s.axis);
  result.radius = s.radius;
  result.minor_radius = s.minor_radius;
  result.half_angle = s.half_angle;
  return result;
}

DatumCurve datum_edge_geometry(const geometry::Shape& shape, rust::Str edge) {
  const geometry::CurveDescription c = geometry::edge_geometry(shape, std::string(edge));
  DatumCurve result;
  result.kind = rust::String(c.type);
  result.start = xyz(c.start);
  result.end = xyz(c.end);
  result.center = xyz(c.center);
  result.normal = xyz(c.normal);
  result.radius = c.radius;
  return result;
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

} // namespace mitcad::bridge
