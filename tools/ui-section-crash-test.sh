#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/main.cpp app/MainWindow.cpp app/commands/InspectCommands.cpp app/framework/CommandSession.cpp core/cpp/bridge/analysis.cpp geometry/src/guard.cpp geometry/analysis/src/section.cpp core/model-api/src/analysis.rs core/ffi/src/kernel/mod.rs tools/cli
# check-all sources: app/MainWindowSection.cpp app/framework/SectionWorker.cpp core/ffi/src/lib.rs
# A crash inside OCCT during Section Analysis is an error, not a crash of
# the application (mitcad#392). Its booleans run off the UI thread, each
# body's on OCCT's thread pool, from the section worker's thread
# (mitcad#438): the clip of each body shown and, after the view has the
# clipped bodies, the section the panel measures. A test hook makes the
# boolean crash as an access violation inside OCCT would
# (MITCAD_TEST_OCCT_CRASH, geometry/src/guard.cpp). On three blocks that
# cross x = 10:
#   1. The clip crashes: the blocks are shown whole (no caps), the panel
#      still gives the section's area, and the app goes on (OK keeps it).
#   2. The section crashes: the blocks are shown cut (the clip worked), then
#      the panel shows the measurement's error, and the app goes on
#      (Escape closes the panel).
#
# Runs headless on Xvfb (see ui-test-lib.sh), without gdb, which would stop
# at the crash that OCCT's handlers turn into an error.
# Usage: tools/ui-section-crash-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-section-crash.XXXXXX)
FILE=$WORK/blocks.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
# The kernel's crashes here are wanted and logged as errors ("SIGSEGV ...
# detected"): the app counts as crashed only when it is gone.
ui_crashed() { ! kill -0 "$UI_RUNNER" 2> /dev/null; }
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Blocks of 30 x 20 x 10 at y = 0, 30 and 60 (F2.b0, F4.b0, F6.b0).
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 30], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [0, 60], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display

# section_at_x10: Section Analysis on YZ, 10 mm along X (each block's
# section there is 20 x 10 mm).
section_at_x10() {
  ui_step "fit (F6)"                         ui_key F6
  ui_expect_log "Bodies shown: " "the blocks shown"
  ui_mark
  ui_step "section analysis (search)"        ui_command "Section Analysis"
  # YZ from the browser: the hidden origin stays hidden (mitcad#1030).
  ui_pick_origin_row YZ
  ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
  ui_step "10 along X"                       ui_type_in "Panel Section Analysis input offset" "10"
}

echo "--- 1. The clip crashes inside OCCT"
UI_GDB=0 MITCAD_TEST_OCCT_CRASH=clip ui_start_app --open "$FILE"
section_at_x10
for body in F2.b0 F4.b0 F6.b0; do
  ui_expect_new "Section: $body shown whole: " "$body, whose clip failed, shown whole"
done
WHOLE=$(grep "Section: F2.b0 shown whole: " "$UI_LOG" | tail -1)
echo "$WHOLE" | grep -qE "clip: SIG(SEGV|BUS)" || ui_fail "the clip's error is not the crash: $WHOLE"
echo "ok   the clip's error is OCCT's crash"
ui_expect_new "Section caps: 0 face(s) at" "no caps on the whole blocks"
ui_expect_new "Inspect Section Analysis: area 600, length 180" "the section still measured"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Closed Section Analysis" "closed, the app still running"
ui_stop_app

echo "--- 2. The section crashes inside OCCT"
: > "$UI_LOG"
UI_GDB=0 MITCAD_TEST_OCCT_CRASH=section ui_start_app --open "$FILE"
section_at_x10
# The panel measures after the view has the clipped blocks (mitcad#438).
# YZ itself (x = 0) is shown and measured first, and its section crashes
# too: only the lines after x = 10's caps count for x = 10.
ui_expect_new "Section caps: 3 face(s) at" "the blocks shown cut at x = 10, the clip worked"
CAPS=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -n "^Section caps: 3 face(s) at" | head -1 | cut -d: -f1)
UI_MARK=$((UI_MARK + CAPS))
ui_expect_new "Inspect Section Analysis: failed: " "then the section's error in the panel"
FAILED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect Section Analysis: failed: " | head -1)
echo "$FAILED" | grep -qE "section: SIG(SEGV|BUS)" || ui_fail "the section's error is not the crash: $FAILED"
echo "ok   the section's error is OCCT's crash, measured after the view's update"
ui_step "cancel (Escape)"                  ui_key Escape
ui_expect_log "Command Section Analysis cancelled" "cancelled, the app still running"

ui_finish "UI section crash test"
