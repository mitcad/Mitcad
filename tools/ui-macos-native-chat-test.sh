#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The AppKit frontend's Chat card (mitcad#1075): the shared chat panel
# through the presenter bridge, with the chat's scripted test agent
# (app/tests/chat_test_agent.cpp) installed and signed in as Claude Code,
# through the real live MCP server and bridge, as tools/ui-chat-test.py runs
# the Qt panel. tools/ui-scripts/native-macos-chat.mitcad-ui: View › Chat,
# the toolbar's Chat button and Command+Shift+L show and hide the card; a
# message adds two boxes as one step that one Command+Z takes back; an
# approval card's Allow once saves the design.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <test_chat_agent> <output>" >&2
  exit 2
fi
if [ ! -x "$2" ]; then
  echo "FAIL: the chat's test agent $2 is missing: build the test_chat_agent target" >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=420
export MITCAD_TEST_WORK=$UI_ROOT/chat
mkdir -p "$MITCAD_TEST_WORK/clients" "$MITCAD_TEST_WORK/designs"
# Claude Code installed and signed in: the scripted agent as `claude`.
cp "$2" "$MITCAD_TEST_WORK/clients/claude"
chmod 755 "$MITCAD_TEST_WORK/clients/claude"
: > "$MITCAD_TEST_WORK/claude-signed-in"
export MITCAD_CHAT_DIR=$MITCAD_TEST_WORK/chat-history
export MITCAD_CHAT_AGENT_DIR=$MITCAD_TEST_WORK/chat-agent
export MITCAD_TEST_ASSISTANT_EXECUTABLES=$MITCAD_TEST_WORK/clients
export MITCAD_ASSISTANT_TEST_WORK=$MITCAD_TEST_WORK
export MITCAD_TEST_CHAT_SAVE_PATH="$MITCAD_TEST_WORK/designs/chat design.mitcad"
export MITCAD_TEST_LOG_URLS=1
export MITCAD_TEST_SHOTS=$UI_OUT
MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-chat.mitcad-ui \
  ui_run_app chat --ui=native --no-recovery --mcp-folder "$MITCAD_TEST_WORK/designs"
ui_expect_log 'TestDriver: done' 'the native chat workflow reached its end'
ui_expect_no_log 'TestDriver: FAIL' 'the native chat steps passed'
ui_expect_log 'Native chat card shown: 340 x [0-9]+' 'the Chat card is 340 points wide in a wide window'
if [ -f "$MITCAD_TEST_CHAT_SAVE_PATH" ]; then
  ui_pass 'Allow once on the approval card saved the design'
else
  ui_fail "the approved save did not write $MITCAD_TEST_CHAT_SAVE_PATH"
fi
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
echo "PASS: the native Chat card: View › Chat, its button and shortcut; one undo step per message; an approval"
