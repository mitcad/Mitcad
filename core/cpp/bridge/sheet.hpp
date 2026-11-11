// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/sheet.rs.

#include <cstddef>
#include <memory>
#include <utility>

#include "bridge/profile.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "mitcad/geometry/sheet.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

struct SheetPatchInput;
struct SheetPort;
struct SheetMember;
struct SheetTrimDomain;
struct SheetRegion;
struct SheetTrimOutput;
struct SheetFootprintCylinder;
struct SheetFootprintFace;

class SheetShapeResult {
public:
  explicit SheetShapeResult(geometry::SheetChartGeometry result) : m_result(std::move(result)) {}

  std::size_t solid_count() const { return m_result.solids.size(); }
  std::shared_ptr<geometry::Shape> solid(std::size_t index) const;
  rust::Vec<SheetMember> membership(std::size_t index) const;
  rust::Vec<rust::String> diagnostics() const;

private:
  geometry::SheetChartGeometry m_result;
};

std::unique_ptr<SheetShapeResult> sheet_shape(rust::Slice<const SheetPatchInput> patches,
                                             rust::Slice<const SheetPort> ports, double thickness,
                                             double tolerance);

SheetTrimOutput sheet_trim(rust::Slice<const SheetTrimDomain> domains, rust::Slice<const SheetRegion> tools,
                          rust::Slice<const SheetPort> seams, std::uint8_t operation, double tolerance);

// The faces of a shape on a bend chart's cylinder, each as its boundary in
// the chart (mitcad#1628).
rust::Vec<SheetFootprintFace> sheet_cylinder_footprint(const geometry::Shape& shape,
                                                      const SheetFootprintCylinder& cylinder, double tolerance);

} // namespace mitcad::bridge
