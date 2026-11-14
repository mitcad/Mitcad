#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/framework/CommandSession.cpp app/MainWindow.cpp
# check-all sources: core/model/src/document/analyses.rs core/model/src/features/geom_ref.rs core/model-api/src/lib.rs
# check-all sources: app/MainWindowSection.cpp core/cpp/bridge/analysis.cpp geometry/analysis/src/section.cpp tools/cli
# Section Analysis through a pick (mitcad#1769) through the real UI, on a
# design mitcad-cli makes: a 40 x 40 x 10 mm block (Body1, F2.b0) with a
# 20 mm hole along Z through its middle (Extrude2, F4; the hole's round
# face F4:side(c1), its axis x = 20, y = 20).
#   1. Two faces selected (the hole's and the top): Section Analysis
#      starts with nothing picked and shows no section.
#   2. The hole's round face selected, then Section Analysis: it starts
#      with the face as its plane and shows the section at once, through
#      the hole's axis, its turn starting at XZ (normal +Y): the block cut
#      across the hole, two 10 x 10 mm pieces (area 200, outline 80).
#   3. OK keeps it as Section1, the face as its plane.
#   4. Section1 edited: its Angle 45 degrees turns the plane about the
#      hole's axis without an Axis (a ring about the hole's axis), through
#      the block's diagonal: (40 sqrt(2) - 20) x 10 = 365.685 mm^2. OK keeps
#      it; the saved file has the face, the angle and no axis.
# The Windows counterpart is tools/ui-windows-section-hole-pick-test.ps1
# (ctest app.ui-windows-section-hole-pick).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-section-hole-pick-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-section-hole-pick.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/hole.mitcad
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
HOLE='F4:side(c1)'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 20], "diameter": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli run"; }

# through_axis nx ny nz: the last section shown since the mark has the
# normal (nx, ny, nz) and contains the hole's axis (x = 20, y = 20, along
# Z): its normal has no Z part and its origin lies on a plane through the
# axis.
through_axis() {
  local line
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Section analysis at " | tail -1)
  [ -n "$line" ] || ui_fail "no section shown"
  python3 - "$line" "$@" << 'EOF' || ui_fail "the section is not through the hole's axis: $line"
import json
import sys

plane = json.loads(sys.argv[1].split("Section analysis at ", 1)[1])["plane"]
origin, normal = plane["origin"], plane["normal"]
want = [float(v) for v in sys.argv[2:5]]
assert all(abs(a - b) < 1e-6 for a, b in zip(normal, want)), normal
assert abs(normal[2]) < 1e-9, normal
assert abs((origin[0] - 20) * normal[0] + (origin[1] - 20) * normal[1]) < 1e-6, origin
EOF
  echo "ok   the section through the hole's axis, normal ($*)"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the block shown"

# Section Analysis once for the log's pick places of the faces; cancelled.
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_expect_new "Command Section Analysis started" "Section Analysis started"
ui_expect_new "Pick face F2.b0/$HOLE at" "the hole's round face can be picked"
ui_expect_new "Pick face F2.b0/F2:end(" "the top face can be picked"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Section Analysis cancelled" "cancelled"

echo "--- 1. Two faces selected: nothing picked"
ui_mark
ui_step "select the hole's face"           ui_click_pick "face F2.b0/$HOLE"
ui_step "Ctrl+click the top face" \
  xdotool mousemove $(ui_pick_at "face F2.b0/F2:end(") keydown ctrl click 1 keyup ctrl
ui_expect_new "Selected: 2 faces" "both faces selected"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_expect_new "Command Section Analysis started" "Section Analysis started"
ui_wait_idle
ui_sync
since=$(tail -n +$((UI_MARK + 1)) "$UI_LOG")
echo "$since" | grep -F "Section Analysis Faces/Plane: 1 " && ui_fail "an item of two selected was taken as the plane"
echo "$since" | grep -F "Inspect Section Analysis: area " && ui_fail "a section was shown"
echo "ok   started with nothing picked, no section"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Section Analysis cancelled" "cancelled"
ui_step "clear the selection (Esc)"        ui_key Escape

echo "--- 2. The hole's face selected, then Section Analysis"
ui_mark
ui_step "select the hole's face"           ui_click_pick "face F2.b0/$HOLE"
ui_expect_new "Selected: 1 face [face $HOLE of F2.b0" "the hole's face selected"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_expect_new "Command Section Analysis started" "Section Analysis started"
ui_expect_new "Section Analysis Faces/Plane: 1 face [face $HOLE of F2.b0" "the selected face is its plane"
ui_expect_new "Section analysis at " "the section shown at once"
ui_expect_new "Inspect Section Analysis: area 200, length 80" \
  "the block cut through the hole's axis: two 10 x 10 mm pieces" 10
through_axis 0 1 0

echo "--- 3. Kept"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Kept Section Analysis as Section1" "kept in the document"
ui_expect_new "Browser Root/Analysis/Section1 at" "listed in the Analysis folder"

echo "--- 4. Its Angle edited: turned about the hole's axis"
ui_double_click_expect "Browser Root/Analysis/Section1" "Editing analysis Section1 with Section Analysis" \
  "its panel opens"
ui_expect_new "Section Analysis Faces/Plane: 1 face [face $HOLE of F2.b0" "with the hole's face"
ui_expect_new "Manipulator Section Analysis angle at" "a ring about the hole's axis, without an Axis"
ui_mark
ui_step "45 degrees"                       ui_type_in "Panel Section Analysis input angle" "45"
ui_expect_new "Inspect Section Analysis: area 365.685" "turned to the block's diagonal: 365.685 mm^2" 10
through_axis -0.7071067811865476 0.7071067811865476 0
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Select an axis for the angle." &&
  ui_fail "the panel asked for an axis"
echo "ok   no axis asked for"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited analysis Section1" "the turned section kept"
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
kept=$(python3 -c 'import json, sys
a = json.load(open(sys.argv[1]))["analyses"][0]
print(a["plane"]["face"], round(a["angle"], 6), "axis" in a)' "$FILE") || ui_fail "no analyses in the file"
[ "$kept" = "$HOLE 0.785398 False" ] || ui_fail "the file's section: $kept"
echo "ok   saved with the hole's face as its plane, the angle and no axis"

ui_finish "UI section hole pick test"
