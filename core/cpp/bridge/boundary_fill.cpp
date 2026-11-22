// SPDX-License-Identifier: MIT
#include "bridge/boundary_fill.hpp"

#include <string>

#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "mitcad_bridge/kernel/boundary_fill.h"

namespace mitcad::bridge {

std::shared_ptr<geometry::Shape> BoundaryCells::cell(std::size_t index) const { return m_cells.at(index).shape; }

rust::Vec<std::size_t> BoundaryCells::cell_holders(std::size_t index) const {
  rust::Vec<std::size_t> holders;
  for (const std::size_t holder : m_cells.at(index).holders) {
    holders.push_back(holder);
  }
  return holders;
}

std::unique_ptr<BoundaryCells> boundary_cells(const ShapeList& tools, rust::Slice<const double> points) {
  std::vector<const geometry::Shape*> shapes;
  for (const auto& tool : tools.items()) {
    shapes.push_back(tool.get());
  }
  std::vector<gp_Pnt> at;
  for (std::size_t i = 0; i + 2 < points.size(); i += 3) {
    at.emplace_back(points[i], points[i + 1], points[i + 2]);
  }
  return std::make_unique<BoundaryCells>(geometry::boundary_cells(shapes, at));
}

std::shared_ptr<geometry::Shape> BoundaryRegionList::region(std::size_t index) const {
  return m_found.regions.at(index).shape;
}

rust::Vec<double> BoundaryRegionList::region_point(std::size_t index) const {
  const gp_Pnt& point = m_found.regions.at(index).point;
  rust::Vec<double> xyz;
  xyz.push_back(point.X());
  xyz.push_back(point.Y());
  xyz.push_back(point.Z());
  return xyz;
}

std::uint8_t BoundaryRegionList::place_kind(std::size_t index) const {
  switch (m_found.places.at(index).kind) {
  case geometry::BoundaryPointPlace::Kind::Region:
    return 0;
  case geometry::BoundaryPointPlace::Kind::OnFace:
    return 1;
  case geometry::BoundaryPointPlace::Kind::Open:
    break;
  }
  return 2;
}

std::size_t BoundaryRegionList::place_region(std::size_t index) const { return m_found.places.at(index).region; }

rust::Vec<double> BoundaryRegionList::place_gap(std::size_t index) const {
  rust::Vec<double> xyz;
  for (const gp_Pnt& point : m_found.places.at(index).gap) {
    xyz.push_back(point.X());
    xyz.push_back(point.Y());
    xyz.push_back(point.Z());
  }
  return xyz;
}

std::unique_ptr<BoundaryRegionList> boundary_regions(const ShapeList& tools, rust::Slice<const double> planes,
                                                     const rust::Vec<rust::String>& plane_names,
                                                     rust::Slice<const double> points) {
  std::vector<const geometry::Shape*> shapes;
  for (const auto& tool : tools.items()) {
    shapes.push_back(tool.get());
  }
  std::vector<gp_Pln> cut;
  for (std::size_t i = 0; i + 8 < planes.size(); i += 9) {
    const gp_Pnt origin(planes[i], planes[i + 1], planes[i + 2]);
    const gp_Dir normal(planes[i + 3], planes[i + 4], planes[i + 5]);
    const gp_Dir x_axis(planes[i + 6], planes[i + 7], planes[i + 8]);
    cut.emplace_back(gp_Ax3(origin, normal, x_axis));
  }
  std::vector<std::string> names;
  for (const rust::String& name : plane_names) {
    names.emplace_back(name);
  }
  std::vector<gp_Pnt> at;
  for (std::size_t i = 0; i + 2 < points.size(); i += 3) {
    at.emplace_back(points[i], points[i + 1], points[i + 2]);
  }
  return std::make_unique<BoundaryRegionList>(geometry::boundary_regions(shapes, cut, names, at));
}

} // namespace mitcad::bridge
