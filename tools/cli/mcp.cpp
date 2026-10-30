// SPDX-License-Identifier: MIT
#include "mcp.hpp"
#include "mcp-connect.hpp"

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
  mitcad-cli mcp --list-instances
      Lists running desktop instances as JSON, without authentication tokens.
  mitcad-cli mcp --connect [--instance ID] [--chat-session MARKER]
      Bridges MCP stdin/stdout to an already-open desktop document.
      Select an ID when several desktop instances are running.
      --chat-session is the desktop chat panel's own (it starts the agent).
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
  std::string instance;
  std::string chat_session;
  bool read_only = false;
  bool connect = false;
  bool list_instances = false;
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
    } else if (args[i] == "--connect") {
      if (connect) {
        return argument_error("--connect may be specified only once");
      }
      connect = true;
    } else if (args[i] == "--list-instances") {
      if (list_instances) {
        return argument_error("--list-instances may be specified only once");
      }
      list_instances = true;
    } else if (args[i] == "--instance") {
      if (!instance.empty()) {
        return argument_error("--instance may be specified only once");
      }
      if (i + 1 == args.size() || args[i + 1].empty() || args[i + 1].rfind("--", 0) == 0) {
        return argument_error("--instance requires an ID");
      }
      instance = args[++i];
    } else if (args[i] == "--chat-session") {
      // The chat panel's marker of its own connection (mitcad#947).
      if (!chat_session.empty()) {
        return argument_error("--chat-session may be specified only once");
      }
      if (i + 1 == args.size() || args[i + 1].empty() || args[i + 1].size() > 64 ||
          args[i + 1].find_first_not_of("0123456789abcdefABCDEF-") != std::string::npos) {
        return argument_error("--chat-session requires the chat's marker");
      }
      chat_session = args[++i];
    } else {
      return argument_error("unknown argument '" + args[i] + "'");
    }
  }
  if (connect || list_instances || !instance.empty() || !chat_session.empty()) {
    if (!workspace.empty() || read_only || (connect && list_instances) ||
        (!connect && (!instance.empty() || !chat_session.empty()))) {
      return argument_error("--connect [--instance ID] and --list-instances are exclusive with headless options");
    }
    return connect ? mcpConnect(instance, chat_session) : mcpListInstances();
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
