// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/surface_sweeps.rs. The curve type is the
// one the sweeps bridge generates.

#include <cstddef>
#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/sweeps.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// geometry::open_faces.
std::shared_ptr<geometry::Shape> open_faces(const geometry::Shape& shape, rust::Str feature,
                                            rust::Slice<const rust::String> roles);
// geometry::extrude_sheet of the chains (`chains[i]` curves each).
std::shared_ptr<geometry::Shape> extrude_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                               rust::Slice<const std::size_t> chains,
                                               rust::Slice<const double> shift,
                                               rust::Slice<const double> direction, double one, double two);
// geometry::revolve_sheet.
std::shared_ptr<geometry::Shape> revolve_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                               rust::Slice<const std::size_t> chains,
                                               rust::Slice<const double> origin,
                                               rust::Slice<const double> direction, double angle1,
                                               double angle2);
// geometry::sweep_sheet.
std::shared_ptr<geometry::Shape> sweep_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                             rust::Slice<const std::size_t> chains,
                                             rust::Slice<const SweepCurve> path, rust::Slice<const double> plane,
                                             rust::Slice<const double> extents, bool parallel);
// geometry::loft_sheet.
std::shared_ptr<geometry::Shape> loft_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                            rust::Slice<const std::size_t> chains, bool ruled, bool closed);
// geometry::helix_sheet (mitcad#1777): the axis as x, y, z, dx, dy, dz.
std::shared_ptr<geometry::Shape> helix_sheet(rust::Str feature, rust::Slice<const SweepCurve> curves,
                                             rust::Slice<const std::size_t> chains, rust::Slice<const double> axis,
                                             double pitch, double revolutions, bool left_handed);

} // namespace mitcad::bridge
