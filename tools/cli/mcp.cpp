// SPDX-License-Identifier: MIT
#include "mcp.hpp"
#include "mcp-connect.hpp"
#include "mcp-raytrace.hpp"

#include <filesystem>
#include <iostream>
#include <sstream>
#include <string>
#include <system_error>

#include <QCoreApplication>
#include <QLoggingCategory>
#include <QString>

#include "mitcad_bridge/lib.h"

namespace mitcad::cli {

const char* const kMcpUsage = R"(  mitcad-cli mcp --workspace DIR [--read-only] [--memory-limit MiB]
      Serves the Model Context Protocol over stdin/stdout without a UI.
      DIR must be an existing directory; document and export paths stay
      inside it. --read-only permits opening, querying and closing designs.
      stdout holds only JSON-RPC messages; diagnostics go to stderr.
      A model command that would run the server out of memory stops with
      an error result that says so, the document as it was before it, at
      85 % of the tightest memory limit of the process (the memory the
      system has left, a cgroup's, ulimit's or a job's limit), or of
      --memory-limit (MiB of resident memory) when that is lower, or
      when its memory grows so fast that it would get there within 0.1 s.
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
  // The model commands' own memory limit in MiB; 0 for none (mitcad#1480).
  double memory_limit = 0.0;
  bool memory_limit_given = false;
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
    } else if (args[i] == "--memory-limit") {
      if (memory_limit_given) {
        return argument_error("--memory-limit may be specified only once");
      }
      if (i + 1 == args.size()) {
        return argument_error("--memory-limit requires a number of MiB");
      }
      std::istringstream text(args[++i]);
      double value = 0.0;
      char rest = 0;
      if (!(text >> value) || text >> rest || !(value > 0.0)) {
        return argument_error("--memory-limit requires a positive number of MiB");
      }
      memory_limit = value;
      memory_limit_given = true;
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
    if (!workspace.empty() || read_only || memory_limit_given || (connect && list_instances) ||
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
  // model_render's ray-traced pictures (mitcad#1489) run the render worker
  // in a Qt event loop; Qt's messages go to stderr.
  int argc = 1;
  char name[] = "mitcad-cli";
  char* argv[] = {name, nullptr};
  QCoreApplication app(argc, argv);
  QLoggingCategory::setFilterRules(QStringLiteral("*.debug=false"));
  mitcad::run_mcp_raytraced(workspace, read_only, memory_limit, makeMcpRaytracer());
  return 0;
}

} // namespace mitcad::cli
