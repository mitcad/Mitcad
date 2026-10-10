// SPDX-License-Identifier: MIT
#include "mcp.hpp"

#include <filesystem>
#include <iostream>
#include <string>
#include <system_error>

#include "mitcad_bridge/lib.h"

namespace mitcad::cli {

const char* const kMcpUsage = R"(  mitcad-cli mcp --workspace DIR [--read-only]
      Serves the Model Context Protocol over stdin/stdout without a UI.
      DIR must be an existing directory; document and export paths stay
      inside it. --read-only permits opening, querying and closing designs.
      stdout holds only JSON-RPC messages; diagnostics go to stderr.
      See docs/mcp.md for client configuration and modelling examples.
)";

namespace {

int argument_error(const std::string& message) {
  std::cerr << "mitcad-cli mcp: " << message << "\n" << kMcpUsage;
  return 2;
}

} // namespace

int mcp(const std::vector<std::string>& args) {
  if (args.size() == 2 && (args[1] == "--help" || args[1] == "-h")) {
    std::cout << kMcpUsage;
    return 0;
  }
  std::string workspace;
  bool read_only = false;
  for (std::size_t i = 1; i < args.size(); ++i) {
    if (args[i] == "--workspace") {
      if (!workspace.empty()) {
        return argument_error("--workspace may be specified only once");
      }
      if (i + 1 == args.size() || args[i + 1].empty() || args[i + 1].rfind("--", 0) == 0) {
        return argument_error("--workspace requires a directory");
      }
      workspace = args[++i];
    } else if (args[i] == "--read-only") {
      if (read_only) {
        return argument_error("--read-only may be specified only once");
      }
      read_only = true;
    } else {
      return argument_error("unknown argument '" + args[i] + "'");
    }
  }
  if (workspace.empty()) {
    return argument_error("--workspace is required");
  }
  std::error_code error;
  if (!std::filesystem::is_directory(std::filesystem::u8path(workspace), error) || error) {
    return argument_error("workspace must be an existing directory: " + workspace);
  }
  mitcad::run_mcp(workspace, read_only);
  return 0;
}

} // namespace mitcad::cli
