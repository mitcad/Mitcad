#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/agent app/mcp/ChatGate.hpp app/mcp/LiveServer.cpp app/mcp/AssistantSetup.cpp
# check-all sources: app/tests/chat_test_agent.cpp core/mcp/src/chat core/ffi/src/chat.rs tools/cli
# check-all sources: tools/ui-chat-test.py tools/ui-chat-sign-in-again-test.py app/framework/TestDriverWindowFit.cpp
# Sign in Again in the chat panel (mitcad#1585) on a hidden Xvfb display:
# tools/ui-chat-sign-in-again-test.py runs the application with the
# scripted test agent (app/tests/chat_test_agent.cpp) in place of Claude
# Code and Codex and a copy of it as the terminal ($TERMINAL): an account
# error offers Sign in Again beside Check again, its confirmation names
# the commands, the terminal runs the chat's own executable's sign-out and
# sign-in, and Check again runs and passes when the terminal ends; without
# a terminal the button is disabled and How to fix shows the commands with
# Copy; the confirmation's text (scrolling in a short panel, mitcad#1590)
# and its buttons are shown whole in both layouts. Windows runs the same
# script as app.ui-windows-chat-sign-in-again,
# macOS as app.ui-macos-chat-sign-in-again (cmake/SelectionUiTests.cmake),
# with the terminal given as MITCAD_TEST_TERMINAL.
#
# Usage: tools/ui-chat-sign-in-again-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CHAT_AGENT=${UI_CHAT_AGENT:-$(dirname "$UI_APP")/test_chat_agent}
[ -x "$CHAT_AGENT" ] || ui_fail "the chat's test agent is missing: build the test_chat_agent target"
ui_start_display
export LIBGL_ALWAYS_SOFTWARE=1
CHAT_LOG_DIR=${MITCAD_UI_LOG_DIR:-$UI_DATA}/chat-sign-in-again
python3 -I "$(dirname "$0")/ui-chat-sign-in-again-test.py" "$UI_APP" "$CHAT_AGENT" "$CHAT_LOG_DIR" ||
  ui_fail "Sign in Again in the chat panel failed; see $CHAT_LOG_DIR"
ui_finish "UI chat Sign in Again test"
