// SPDX-License-Identifier: MIT
#pragma once

#include <string>

namespace mitcad::cli {

// Discover running GUI instances or bridge MCP stdio to one of them.
int mcpListInstances();
// `chatSession`: the chat panel's marker of its own connection (mitcad#947).
int mcpConnect(const std::string& instance, const std::string& chatSession = {});

} // namespace mitcad::cli
