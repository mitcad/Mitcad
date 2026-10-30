#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The chat panel's group rows of tool calls (mitcad#983): a one-off visual
# check, not a recurring test (its name is not ui-*-test.sh, so no check
# picks it up). The scripted test agent of the chat's UI test (app/tests/
# chat_test_agent.cpp) answers one message with three calls in a row (the
# second fails), a text, two calls and a text, on the light and the dark
# theme (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME):
#   <dir>/chat-groups-<theme>-closed.png  both group rows closed
#   <dir>/chat-groups-<theme>-open.png    the first group opened, its
#                                         failed call opened too
# The app's logs as <dir>/look-<theme>.log.
#
# What to look for: one row per group, "3 calls (1 failed) · document_list"
# and "2 calls · ui_state", their text from the left like the rows inside;
# the opened group's rows indented below its head, the failed one in the
# error colour (readable on both themes) and open to its Input and Result;
# the agent's texts between the groups.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-chat-groups-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-chat-groups-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CHAT_AGENT=${UI_CHAT_AGENT:-$(dirname "$UI_APP")/test_chat_agent}
[ -x "$CHAT_AGENT" ] || ui_fail "the chat's test agent is missing: build the test_chat_agent target"
trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
ui_start_display
export LIBGL_ALWAYS_SOFTWARE=1
python3 -I "$(dirname "$0")/ui-chat-groups-look.py" "$UI_APP" "$CHAT_AGENT" "$DIR" ||
  ui_fail "the chat group pictures failed; see $DIR"
ui_finish "Chat group pictures in $DIR"
