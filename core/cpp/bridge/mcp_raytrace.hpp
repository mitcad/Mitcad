// SPDX-License-Identifier: MIT
#pragma once

// The ray-traced picture of the headless MCP server's model_render
// (mitcad#1489): the Rust server owns the documents and the camera, the
// command line tool renders through the Cycles render worker as
// `mitcad-cli render` does (tools/cli/mcp-raytrace.cpp). The application
// and the other programs that link the bridge pass none.

#include "rust/cxx.h"

namespace mitcad {

struct Document;

class McpRaytracer {
public:
  McpRaytracer() = default;
  McpRaytracer(const McpRaytracer&) = delete;
  McpRaytracer& operator=(const McpRaytracer&) = delete;
  virtual ~McpRaytracer() = default;

  // `request` and the answer are JSON (core/ffi/src/mcp/raytrace.rs): the
  // image, or an error with its error_code. Never throws.
  virtual rust::String render(const Document& document, rust::Str request) const = 0;
};

} // namespace mitcad
