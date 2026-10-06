// SPDX-License-Identifier: MIT
#include "bridge/persist.hpp"

#include <string_view>

#include "mitcad/geometry/persist.hpp"
#include "mitcad_bridge/kernel/persist.h"

namespace mitcad::bridge {

std::unique_ptr<std::string> shape_bytes(const geometry::Shape& shape) {
  return std::make_unique<std::string>(geometry::serialize_shape(shape));
}

std::shared_ptr<geometry::Shape> shape_from_bytes(rust::Slice<const std::uint8_t> data) {
  return geometry::deserialize_shape(
      std::string_view(reinterpret_cast<const char*>(data.data()), data.size()));
}

std::uint64_t shape_memory(const geometry::Shape& shape) { return geometry::memory_estimate(shape); }

ProcessMemory process_memory() {
  const geometry::ProcessMemory memory = geometry::process_memory();
  ProcessMemory out{};
  out.resident = memory.resident;
  out.peak_resident = memory.peak_resident;
  out.heap = memory.heap;
  return out;
}

} // namespace mitcad::bridge
