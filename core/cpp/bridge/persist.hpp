// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/persist.rs: shapes as bytes for the
// result store (P7d), and the memory the caches' diagnostics show.

#include <cstdint>
#include <memory>
#include <string>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/persist.h.
struct ProcessMemory;

std::unique_ptr<std::string> shape_bytes(const geometry::Shape& shape);
std::shared_ptr<geometry::Shape> shape_from_bytes(rust::Slice<const std::uint8_t> data);
std::uint64_t shape_memory(const geometry::Shape& shape);
ProcessMemory process_memory();

} // namespace mitcad::bridge
