// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/boundary_fill.rs.

#include <cstddef>
#include <cstdint>
#include <memory>
#include <utility>
#include <vector>

#include "bridge/shape.hpp"
#include "mitcad/geometry/boundary_fill.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// The cells of geometry::boundary_cells, in the order of the points.
class BoundaryCells {
public:
  explicit BoundaryCells(std::vector<geometry::BoundaryCell> cells) : m_cells(std::move(cells)) {}

  std::size_t cell_count() const { return m_cells.size(); }
  std::shared_ptr<geometry::Shape> cell(std::size_t index) const;
  rust::Vec<std::size_t> cell_holders(std::size_t index) const;

private:
  std::vector<geometry::BoundaryCell> m_cells;
};

// geometry::boundary_cells of the tools at the points (x, y, z of each in
// turn).
std::unique_ptr<BoundaryCells> boundary_cells(const ShapeList& tools, rust::Slice<const double> points);

// The regions of geometry::boundary_regions (mitcad#2149) and where each
// point asked about lies.
class BoundaryRegionList {
public:
  explicit BoundaryRegionList(geometry::BoundaryRegions found) : m_found(std::move(found)) {}

  std::size_t region_count() const { return m_found.regions.size(); }
  std::shared_ptr<geometry::Shape> region(std::size_t index) const;
  // x, y, z of the point inside the region.
  rust::Vec<double> region_point(std::size_t index) const;
  std::size_t place_count() const { return m_found.places.size(); }
  // 0: in the region place_region gives, 1: on a face, 2: in an open region.
  std::uint8_t place_kind(std::size_t index) const;
  std::size_t place_region(std::size_t index) const;
  // x, y, z of each point along the gap of an open region in turn.
  rust::Vec<double> place_gap(std::size_t index) const;

private:
  geometry::BoundaryRegions m_found;
};

// geometry::boundary_regions of the tools, the planes (origin, normal and x
// axis of each in turn, 9 numbers) and their faces' names, at the points
// (x, y, z of each in turn).
std::unique_ptr<BoundaryRegionList> boundary_regions(const ShapeList& tools, rust::Slice<const double> planes,
                                                     const rust::Vec<rust::String>& plane_names,
                                                     rust::Slice<const double> points);

} // namespace mitcad::bridge
