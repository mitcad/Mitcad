// SPDX-License-Identifier: MIT
#pragma once

#include <string>
#include <vector>

namespace mitcad::cli {

// The headless MCP stdio server (mitcad#124, docs/mcp.md).
extern const char* const kMcpUsage;
int mcp(const std::vector<std::string>& args);

} // namespace mitcad::cli
