#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/agent app/mcp/ChatGate.hpp app/mcp/LiveServer.cpp app/mcp/AgentIndicator.cpp
# check-all sources: app/mcp/ConnectAssistantDialog.cpp app/framework/FloatingLayout.cpp app/framework/TitleBar.cpp
# check-all sources: app/tests/chat_test_agent.cpp core/mcp/src/chat core/ffi/src/chat.rs tools/cli tools/ui-chat-test.py
# The chat panel (mitcad#947) on a hidden Xvfb display: tools/ui-chat-test.py
# runs the application with the scripted test agent (app/tests/
# chat_test_agent.cpp) in place of Claude Code and Codex, through the real
# live MCP server and mitcad-cli bridge: setup (not installed, installed
# meanwhile, sign-in), the privacy notice, one undo step per message, the
# approval cards, a user's change during a turn, Stop, the history of a
# reopened design and its resumed session, the Docked layout, Claude
# Desktop only, the model choice, and Codex through its app server chosen
# in the agent switcher (kept as the last used one).
# Windows runs the same script as app.ui-windows-chat, macOS as
# app.ui-macos-chat (cmake/SelectionUiTests.cmake).
#
# Usage: tools/ui-chat-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CHAT_AGENT=${UI_CHAT_AGENT:-$(dirname "$UI_APP")/test_chat_agent}
[ -x "$CHAT_AGENT" ] || ui_fail "the chat's test agent is missing: build the test_chat_agent target"
ui_start_display
export LIBGL_ALWAYS_SOFTWARE=1
CHAT_LOG_DIR=${MITCAD_UI_LOG_DIR:-$UI_DATA}/chat
python3 -I "$(dirname "$0")/ui-chat-test.py" "$UI_APP" "$CHAT_AGENT" "$CHAT_LOG_DIR" ||
  ui_fail "the chat panel failed; see $CHAT_LOG_DIR"
ui_finish "UI chat panel test"
