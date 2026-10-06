// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/exchange.rs.

#include <cstddef>
#include <cstdint>
#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::f3d {
// Defined in the generated mitcad_bridge/brep_import.h.
struct BrepBodyData;
} // namespace mitcad::f3d

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/exchange.h.
enum class BodyKind : std::uint8_t;
struct BodyInfo;
struct BodyPlacement;
struct ExportSettings;
struct F3dBody;

std::shared_ptr<geometry::Shape> import_brep(rust::Str feature, rust::Slice<const std::uint8_t> data,
                                             std::uint32_t first_face);
std::size_t face_count(const geometry::Shape& shape);
rust::Vec<std::uint8_t> brep_data(const geometry::Shape& shape);
std::shared_ptr<geometry::Shape> compound(const ShapeList& shapes);
BodyKind body_kind(const geometry::Shape& shape);

rust::Vec<BodyInfo> read_file(rust::Str path, double unit_mm, ShapeList& shapes);
void write_file(rust::Str path, const ShapeList& shapes, rust::Slice<const BodyInfo> bodies,
                const ExportSettings& settings);

rust::Vec<F3dBody> f3d_bodies(rust::Str path, bool history, bool owners, ShapeList& shapes);

// .f3d import (T1).
std::shared_ptr<geometry::Shape> f3d_build_body(const f3d::BrepBodyData& data);
void catch_occt_crashes();

} // namespace mitcad::bridge
