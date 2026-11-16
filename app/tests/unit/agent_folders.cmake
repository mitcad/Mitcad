# SPDX-License-Identifier: MIT
# The folders for designs, which live MCP agents save to and open from (mitcad#402):
# plain Qt Core and Widgets.
target_sources(test_app PRIVATE tests/agent_folders.cpp mcp/AgentFolders.cpp mcp/AgentFolders.hpp)
