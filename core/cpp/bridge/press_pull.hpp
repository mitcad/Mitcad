// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/press_pull.rs (mitcad#1850).

#include <memory>

#include "mitcad/geometry/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/press_pull.h.
struct PressPullParams;
struct FaceRunInfo;

// geometry::press_pull.
std::shared_ptr<geometry::Shape> press_pull(rust::Str feature, const geometry::Shape& body,
                                            rust::Slice<const rust::String> faces, const PressPullParams& params);

// geometry::face_run.
FaceRunInfo face_run(const geometry::Shape& body, rust::Str face);

} // namespace mitcad::bridge
