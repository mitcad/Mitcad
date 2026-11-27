// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/surface_fills.rs. The curve type is the
// one its bridge generates.

#include <cstdint>
#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/sweeps.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/surface_fills.h.
struct BlendEndData;
struct BoundaryData;
struct ChainData;

// geometry::blend_surface.
std::shared_ptr<geometry::Shape> blend_surface(rust::Str feature, const ShapeList& bodies, const BlendEndData& from,
                                               const BlendEndData& to);
// geometry::boundary_fill (fill: the loop in u, its guide curves in
// guides) and geometry::boundary_network.
std::shared_ptr<geometry::Shape> boundary_surface(rust::Str feature, const ShapeList& bodies,
                                                  rust::Slice<const SweepCurve> curves, bool fill,
                                                  rust::Slice<const BoundaryData> u,
                                                  rust::Slice<const BoundaryData> v,
                                                  rust::Slice<const BoundaryData> guides);
// geometry::match_surface.
std::shared_ptr<geometry::Shape> match_surface(const geometry::Shape& body, rust::Str face, const ShapeList& bodies,
                                               const ChainData& target, std::uint8_t continuity);
// geometry::fair_surface.
std::shared_ptr<geometry::Shape> fair_surface(const geometry::Shape& body, rust::Slice<const rust::String> faces,
                                              double strength, std::uint32_t iterations, bool keep_edges,
                                              double tolerance);

} // namespace mitcad::bridge
