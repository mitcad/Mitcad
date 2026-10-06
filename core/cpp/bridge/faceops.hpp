// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/faceops.rs. The profile types are the
// ones the profile bridge generates.

#include <memory>

#include "bridge/shape.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/profile.h"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated mitcad_bridge/kernel/faceops.h.
struct ToolSpec;
struct ShellParams;
struct DraftParams;

std::shared_ptr<geometry::Shape> shell(rust::Str feature, const geometry::Shape& body,
                                       rust::Slice<const rust::String> faces,
                                       const ShellParams& params);
std::shared_ptr<geometry::Shape> draft(rust::Str feature, const geometry::Shape& body,
                                       rust::Slice<const rust::String> faces, const ToolSpec& plane,
                                       const ShapeList& plane_shapes, const DraftParams& params);
std::shared_ptr<geometry::Shape> offset_faces(rust::Str feature, const geometry::Shape& body,
                                              rust::Slice<const rust::String> faces,
                                              double distance);
std::shared_ptr<geometry::Shape> delete_faces(rust::Str feature, const geometry::Shape& body,
                                              rust::Slice<const rust::String> faces);
std::shared_ptr<geometry::Shape> replace_faces(rust::Str feature, const geometry::Shape& body,
                                               rust::Slice<const rust::String> faces,
                                               const ToolSpec& target,
                                               const ShapeList& target_shapes, bool tangent_chain);
std::unique_ptr<ShapeList> split_body(rust::Str feature, const geometry::Shape& body,
                                      const ToolSpec& tool, const ShapeList& tool_shapes,
                                      const Frame& frame, rust::Slice<const Region> regions);
std::shared_ptr<geometry::Shape> split_faces(rust::Str feature, const geometry::Shape& body,
                                             rust::Slice<const rust::String> faces,
                                             const ToolSpec& tool, const ShapeList& tool_shapes,
                                             const Frame& frame, rust::Slice<const Region> regions);

} // namespace mitcad::bridge
