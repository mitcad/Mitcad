// SPDX-License-Identifier: MIT
#pragma once

// model_render's ray-traced picture on the headless MCP server
// (mitcad#1489, mcp-raytrace.cpp).

#include <memory>

#include "bridge/mcp_raytrace.hpp"

namespace mitcad::cli {

// The raytracer of `mitcad-cli mcp --workspace`: the scene and the job of
// `mitcad-cli render` (render-scene.hpp, app/render/RenderBatch) through the
// render worker mitcad-render. Needs a QCoreApplication.
std::unique_ptr<McpRaytracer> makeMcpRaytracer();

} // namespace mitcad::cli
