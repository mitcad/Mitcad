#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeMacWindow.mm app/platform/macos/NativeDialogs.mm app/MainWindowNative.cpp
# check-all sources: app/MainWindowStart.cpp app/files/StartPage.cpp app/browser/TimelineWidget.cpp app/agent/ChatPanel.cpp
# check-all sources: app/browser/TimelineWidget.hpp app/platform/NativeMacWindow.hpp
# check-all sources: app/framework/TestDriver.cpp app/platform/macos/NativeTestInput.mm
# check-all sources: tools/ui-scripts/native-macos-minor-gaps.mitcad-ui tools/ui-scripts/native-macos-minor-gaps-start.mitcad-ui
# check-all sources: tools/ui-scripts/native-macos-minor-gaps-chat.mitcad-ui tools/cli
# Seven small AppKit gaps (mitcad#1358), each checked in the AppKit
# frontend through the TestDriver's in-process native commands (no system
# input), in three starts of MitCad:
#   1. minor-gaps, on a design mitcad-cli makes (Sketch1, Block its extrude,
#      Block round a failed rounding, the components Pin and Cap with a post
#      each, the Center of Mass marker): the window's title carries the
#      version, as the Qt window's does; timeline search's results show the
#      feature's icon, a failed one in the error colour, its state in the
#      tooltip and its component's swatch (mitcad#1380), none for the root's;
#      the Browser's Delete key removes the marker while Sketch1 is edited
#      (tools/ui-scripts/native-macos-minor-gaps.mitcad-ui);
#   2. minor-gaps-start, on the Start tab at launch: no empty row of groups
#      above the Start page, its footer above a shown message; the Import
#      STEP Assembly answers' tooltips in the native alert
#      (tools/ui-scripts/native-macos-minor-gaps-start.mitcad-ui);
#   3. minor-gaps-chat, with two Claude Codes (the chat's scripted test
#      agent; the first signed in, the one in ~/.local/bin not): the agent
#      pop-up shows each one's sign-in state under its name and in its
#      tooltip (tools/ui-scripts/native-macos-minor-gaps-chat.mitcad-ui).
# Usage: tools/ui-macos-native-minor-gaps-test.sh <app> <mitcad-cli> <test_chat_agent> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> <test_chat_agent> <output directory>" >&2
  exit 2
fi
cli=$2
agent=$3
if [ ! -x "$agent" ]; then
  echo "FAIL: the chat's test agent $agent is missing: build the test_chat_agent target" >&2
  exit 2
fi
ui_init "$1" "$4"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_ROOT/files
export MITCAD_TEST_SHOTS=$UI_OUT
mkdir -p "$MITCAD_TEST_WORK"
setup() {
  "$cli" "$@" >> "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; echo "FAIL: mitcad-cli $*"; exit 1; }
}
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$MITCAD_TEST_WORK/design.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "name": "Block", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "name": "Block round", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side0|F2:side1}"], "radius": 100}},
  {"expect": {"feature": "Block round", "status": "error"}},
  {"cmd": "create_component", "name": "Pin"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [50, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "name": "Pin post", "def": {"type": "extrude", "profiles": [{"sketch": "F4", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Cap"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [80, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "name": "Cap post", "def": {"type": "extrude", "profiles": [{"sketch": "F6", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "add_center_of_mass"}
]
EOF
setup run "$MITCAD_TEST_WORK/design.json" --save "$MITCAD_TEST_WORK/design.mitcad"
# A STEP assembly that places one part twice, as tools/ui-import-test.sh
# makes it (mitcad#1274).
cat > "$MITCAD_TEST_WORK/pins.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 10, "height": 5, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Pin"},
  {"cmd": "components_from_bodies", "bodies": ["F1.b0"]},
  {"cmd": "copy_occurrence", "occurrence": "Pin:1", "transform": {"translation": [0, 50, 0]}},
  {"cmd": "export", "path": "$MITCAD_TEST_WORK/pins.step"}
]
EOF
setup run "$MITCAD_TEST_WORK/pins.json"
ui_pass "the design and the STEP assembly are made"

MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-minor-gaps.mitcad-ui \
  ui_run_app minor-gaps --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/design.mitcad"
ui_expect_log '^TestDriver: done' 'minor-gaps: the steps'
ui_expect_log '^Deleted the Center of Mass marker' 'the Delete key removed the marker in Sketch mode'
ui_expect_log '^Native search swatch native[.]timelineSearchResult[.]F2: none$' "no swatch on a root feature's result"
ui_expect_log '^Native search swatch native[.]timelineSearchResult[.]F5: #' "Pin post's result has its component's swatch"
ui_expect_log '^Native search swatch native[.]timelineSearchResult[.]F7: #' "Cap post's result has its component's swatch"

MITCAD_START_PAGE=1 MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-minor-gaps-start.mitcad-ui \
  ui_run_app minor-gaps-start --ui=native --no-recovery --no-mcp
ui_expect_log '^TestDriver: done' 'minor-gaps-start: the steps'
ui_expect_log '^Import STEP Assembly: cancelled$' 'the STEP assembly question was answered in the native alert'

# Two Claude Codes, as tools/ui-chat-test.py sets them up (mitcad#1289):
# the scripted test agent as `claude`, signed in as first@example.com, and
# a copy in the test home's ~/.local/bin, signed out.
chat=$UI_ROOT/chat
mkdir -p "$chat/clients" "$chat/designs" "$chat/home/.local/bin"
cp "$agent" "$chat/clients/claude"
cp "$agent" "$chat/home/.local/bin/claude"
chmod 755 "$chat/clients/claude" "$chat/home/.local/bin/claude"
printf 'first@example.com' > "$chat/clients/claude-account"
printf 'signed-out' > "$chat/home/.local/bin/claude-account"
# The API agents' keys stay out of the user's keychain, as in the Qt test.
unset ANTHROPIC_API_KEY
export MITCAD_TEST_KEYCHAIN=off
MITCAD_CHAT_DIR=$chat/chat-history MITCAD_CHAT_AGENT_DIR=$chat/chat-agent \
  MITCAD_TEST_ASSISTANT_EXECUTABLES=$chat/clients MITCAD_TEST_ASSISTANT_HOME=$chat/home \
  MITCAD_ASSISTANT_TEST_WORK=$chat MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-minor-gaps-chat.mitcad-ui \
  ui_run_app minor-gaps-chat --ui=native --no-recovery --mcp-folder "$chat/designs"
ui_expect_log '^TestDriver: done' 'minor-gaps-chat: the steps'

for log in "$UI_OUT"/minor-gaps*.log; do
  UI_LOG_FILE=$log
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
    ui_expect_no_log "$problem" "$(basename "$log"): no $problem"
  done
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: the minor AppKit gaps of mitcad#1358'
