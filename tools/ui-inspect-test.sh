#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands geometry/analysis
# The INSPECT group (U4) through the real UI, on two 20 mm cubes that
# overlap by 5 mm (Body1 at x = 0, Body2 at x = 15):
#   - Measure between a face and a vertex, and an edge's length; in sketch
#     mode between two sketch lines (P9).
#   - Interference: the overlap's volume.
#   - Section Analysis at the YZ plane moved by its arrow and typed: the
#     section's area; kept after OK until Remove Section Analysis.
#   - The analysis kept in the document (mitcad#41): the browser's Analysis
#     folder lists it; Remove Section Analysis hides it, its light bulb
#     shows and hides it; double-click edits it; saved with the file and
#     cut again when it opens; deleted from the context menu, and undone.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-inspect-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-inspect.XXXXXX)
FILE=$WORK/cubes.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/cubes.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [15, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/cubes.json" --save "$WORK/cubes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$WORK/cubes.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Measure"
ui_step "measure (I)"                      ui_key i
ui_expect_log "Command Measure started" "measure started"
ui_step "pick Body2's top face"            ui_click_pick "face F4.b0/F4:end($R)"
ui_expect_log "Measure Selection 1: 1 face" "a face picked"
ui_expect_log "Inspect Measure: area 400" "its area"
ui_step "pick a bottom vertex"             ui_click_pick "vertex F4.b0/V{F4:side(c1[c4,c2])|F4:side(c2[c1,c3])|F4:start("
ui_expect_log "Measure Selection 2: 1 vertex" "a vertex picked"
ui_expect_log "distance 20" "the top is 20 mm above it"
ui_step "clear the second"                 ui_key Escape
ui_expect_log "Command Measure cancelled" "measure closed"
ui_step "measure (I)"                      ui_key i
ui_step "pick an edge"                     ui_click_pick "edge F4.b0/E{F4:end($R)|F4:side(c1[c4,c2])}"
ui_expect_log "Inspect Measure: length 20" "the edge is 20 long"
ui_step "close (Enter)"                    ui_key Return
ui_expect_log "Closed Measure" "measure closed"

echo "--- Measure in sketch mode (P9)"
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
ui_step "edit Sketch1 (double-click)"      ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "Measure starts in sketch mode"
ui_expect_new "Sketch entity F1/c1 at" "it takes the sketch's curves"
ui_step "pick the bottom line"             ui_click_logged "Sketch entity F1/c1"
ui_expect_new "Inspect Measure: length 20" "a sketch line's length"
ui_step "pick the top line"                ui_click_logged "Sketch entity F1/c3"
ui_expect_new "distance 20, angle 0 deg" "20 mm apart and parallel"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed, still sketching"
ui_step "finish the sketch (Ctrl+Enter)"   ui_key ctrl+Return
ui_expect_new "Sketch finished" "back in the model"

echo "--- Interference"
ui_step "interference (search)"            ui_command "Interference"
ui_expect_log "Inspect Interference: Body1 x Body2: 2000" "they overlap by 5 x 20 x 20"
ui_step "close (Enter)"                    ui_key Return

echo "--- Section Analysis"
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_expect_log "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
ui_expect_log "Section analysis at" "the bodies are shown cut"
ui_expect_log "Manipulator Section Analysis offset at" "the plane has an arrow"
ui_mark
ui_step "drag the arrow"                   ui_drag_from "Manipulator Section Analysis offset" 60 0
ui_expect_new "Manipulator Section Analysis offset dragged to" "arrow dragged"
ui_step "10 along X"                       ui_type_in "Panel Section Analysis input offset" "10"
ui_expect_log "Inspect Section Analysis: area 400, length 80" "the cut through Body1 at x = 10"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Closed Section Analysis" "closed, the section kept"
ui_expect_log "Kept Section Analysis as Section1" "kept in the document"
ui_expect_log 'Section analysis at {"plane":{"normal":[1,0,0],"origin":[10,0,0]}}' "cut where the model says"
grep "Section analysis off" "$UI_LOG" > /dev/null && ui_fail "the section went with the panel"
ui_expect_log "Browser Root/Analysis/Section1 at" "the browser's Analysis folder lists it"
ui_mark
ui_step "remove section analysis (search)" ui_command "Remove Section Analysis"
ui_expect_new "Section analysis off" "the bodies are whole again"

echo "--- The Analysis folder (mitcad#41)"
ui_mark
ui_step "show it (light bulb)"             ui_click_logged "Browser eye Root/Analysis/Section1"
ui_expect_new "Visibility Root/Analysis/Section1: shown" "shown with its light bulb"
ui_expect_new '"origin":[10,0,0]' "cut again"
ui_mark
ui_step "hide it (light bulb)"             ui_click_logged "Browser eye Root/Analysis/Section1"
ui_expect_new "Visibility Root/Analysis/Section1: hidden" "hidden with its light bulb"
ui_expect_new "Section analysis off" "whole again"
ui_mark
ui_step "edit it (double-click)"           ui_double_click_logged "Browser Root/Analysis/Section1"
ui_expect_new "Editing analysis Section1 with Section Analysis" "its panel opens"
ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "with its plane"
ui_expect_new "Inspect Section Analysis: area 400, length 80" "and its offset"
ui_step "18 along X"                       ui_type_in "Panel Section Analysis input offset" "18"
ui_expect_new "Inspect Section Analysis: area 800, length 160" "the cut through both cubes at x = 18"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited analysis Section1" "the analysis changed"
ui_expect_new '"origin":[18,0,0]' "shown at its new place"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
analyses=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["analyses"])' "$FILE") ||
  ui_fail "no analyses in the file"
[ "$analyses" = "[{'name': 'Section1', 'type': 'section', 'plane': 'yz', 'offset': 18.0}]" ] ||
  ui_fail "the file's analyses: $analyses"
echo "ok   saved in the file"
ui_stop_app
ui_start_app --open "$FILE"
ui_expect_log 'Section analysis at {"plane":{"normal":[1,0,0],"origin":[18,0,0]}}' "cut again when the file opens"
ui_expect_log "Browser Root/Analysis/Section1 at" "listed again"
ui_mark
ui_step "right-click Section1"             ui_click_logged "Browser Root/Analysis/Section1" 3
ui_expect_new "Context menu: Edit Section Analysis | Hide | Rename | Delete" "its menu"
ui_step "choose Delete"                    ui_menu_choose "Delete"
ui_expect_new "Deleted analysis Section1" "deleted"
ui_expect_new "Section analysis off" "whole again"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Delete Section1" "undone"
ui_expect_new '"origin":[18,0,0]' "cut again"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI inspect test"
