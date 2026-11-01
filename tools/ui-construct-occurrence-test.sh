#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/ModelShapes.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowActivate.cpp core/model/src/features/construction tools/cli
# A construction plane of the active component on a face of another
# component's occurrence, both placed by a turn and a move, through the
# real UI (mitcad#1029). Motor:1 (O1) is turned 90 degrees about Z and
# moved by (100, 0, 0): its 30 x 20 x 10 block's top face (z = 10 in Motor)
# lies at z = 10 in the design too. Frame:1 (O2) is turned 90 degrees
# about X and moved by (0, 50, 0): a point q of the design is (qx, qz,
# 50 - qy) in Frame.
#   - Frame active, Offset Plane on Motor's top face: the pick asks
#     whether to activate Motor:1 (mitcad#960), answered Keep in Frame:1.
#     The preview is drawn where the finished plane is shown ("Preview
#     datum in the design" against the Construct command's "Added …; in
#     the design: …"): 10 mm above the face, z = 20 with the normal
#     (0, 0, 1) in the design, y = 20 with the normal (0, 1, 0) in Frame.
#   - The plane edited from the timeline: its preview there too, 12 mm
#     above the face; cancelled.
#   - The design saved: the plane is Frame's, linked to Motor's body from
#     O1 to O2, and moving Motor:1 15 mm up (mitcad-cli) moves it 15 mm
#     along Frame's y.
# The Windows test is tools/ui-windows-construct-occurrence-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-construct-occurrence-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"
# The question whether to activate a picked face's component (mitcad#960),
# which the other tests turn off.
export MITCAD_ASK_ACTIVATE_COMPONENT=1

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-construct-occurrence.XXXXXX)
FILE=$WORK/turned.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Motor (C1, O1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: F2.b0.
# Frame (C2, O2): Sketch2 (F3) 20 x 20, Extrude2 (F4) 5 up: F4.b0, clear of
# Motor's block in the design. The plane made in the test is F5.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Motor", "transform": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Frame", "transform": [[1, 0, 0, 0], [0, 0, -1, 50], [0, 1, 0, 0]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# part "origin (x, y, z), normal (a, b, c)" origin|normal: the three
# numbers, separated by spaces.
part() {
  sed -n "s/.*$2 (\([^)]*\)).*/\1/p" <<< "$1" | tr -d ','
}

# within "x y z" "ex ey ez" tolerance description: each coordinate within
# the tolerance of the expected one; "-" expects any value.
within() {
  awk -v got="$1" -v want="$2" -v tol="$3" 'BEGIN {
    if (split(got, g, / +/) != 3 || split(want, w, / +/) != 3) exit 1
    for (i = 1; i <= 3; i++) {
      if (w[i] == "-") continue
      d = g[i] - w[i]; if (d < 0) d = -d; if (d > tol) exit 1
    }
  }' || ui_fail "$4: ($1), expected ($2) within $3"
  echo "ok   $4: ($1)"
}

# last_new text: the last line with the text since the mark.
last_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the bodies shown"
ui_mark
ui_step "activate Frame (radio)"         ui_click_logged "Browser radio Root/Frame:1"
ui_expect_new "Activated Frame" "Frame is active"

echo "--- Offset Plane in Frame on Motor:1's top face, kept in Frame:1"
ui_mark
ui_step "offset plane (search)"          ui_command "Offset Plane"
ui_expect_new "Pick face F2.b0/F2:end(" "Motor's top face can be picked"
ui_step "pick Motor's top face"          ui_click_pick "face F2.b0/F2:end("
ui_expect_new "Activate component asked: Motor:1 (active " "asked whether to activate Motor:1"
ui_focus_dialog '^Activate Component$'
ui_step "keep in Frame:1 (Alt+K)"        ui_key alt+k
ui_focus_main
ui_expect_new "Activate component answer: keep in " "kept in Frame"
ui_expect_new "Offset Plane Plane: 1 face" "the face picked"
ui_expect_new "Preview Offset Plane: ok" "previewed"
ui_expect_new "Preview datum in the design: origin (" "the preview's plane logged"
preview=$(last_new "Preview datum in the design: ")
preview=${preview#*Preview datum in the design: }
within "$(part "$preview" origin)" "- - 20" 1e-6 "the preview 10 mm above the face in the design"
within "$(part "$preview" normal)" "0 0 1" 1e-6 "the preview's normal the face's"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added plane Plane1: origin (" "Plane1 added"
added=$(last_new "Added plane Plane1: ")
case "$added" in *"; in the design: "*) ;; *) ui_fail "Plane1 is not logged in the design: $added" ;; esac
own=${added#*Added plane Plane1: }
own=${own%%; in the design: *}
design=${added#*; in the design: }
[ "$design" = "$preview" ] || ui_fail "the preview ($preview) is not where Plane1 is shown ($design)"
echo "ok   the preview where Plane1 is shown: $design"
within "$(part "$own" origin)" "- 20 -" 1e-6 "Plane1 20 mm along Frame's y"
within "$(part "$own" normal)" "0 1 0" 1e-6 "Plane1's normal Frame's y"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Plane1 edited from the timeline: its preview where it is shown"
ui_mark
ui_step "double-click Plane1"            ui_double_click_logged "Timeline Plane1"
ui_expect_new "Editing F5 with Offset Plane" "editing Plane1"
ui_step "type 12"                        ui_type_in "Panel Offset Plane input distance" "12"
ui_expect_new "Preview Offset Plane: ok" "previewed"
ui_expect_new "Preview datum in the design: origin (" "the edited preview's plane logged"
edited=$(last_new "Preview datum in the design: ")
edited=${edited#*Preview datum in the design: }
within "$(part "$edited" origin)" "$(part "$design" origin | awk '{ print $1, $2, $3 + 2 }')" 1e-4 \
  "the edited preview 2 mm above Plane1 in the design"
within "$(part "$edited" normal)" "0 0 1" 1e-6 "the edited preview's normal"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Offset Plane cancelled" "the edit cancelled"

echo "--- Saved: Plane1 linked to Motor's body; Motor:1 moved 15 mm up"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
moved=$(part "$own" origin | awk '{ print $1 ", " $2 + 15 ", " $3 }')
cat > "$WORK/moved.json" << EOF
[
  {"expect": {"query": {"query": "feature", "uid": "F5"},
    "result": {"status": "ok", "component": "C2",
               "def": {"links": {"F2.b0": {"source": "O1", "target": "O2"}}}}}},
  {"cmd": "set_occurrence_transform", "occurrence": "O1",
    "transform": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 15]]},
  {"expect": {"query": {"query": "datum", "uid": "F5"},
    "result": {"origin": [$moved], "normal": [0, 1, 0]}, "tolerance": 1e-4}}
]
EOF
"$CLI" run "$WORK/moved.json" --open "$FILE" > "$WORK/cli-moved.log" 2>&1 ||
  { cat "$WORK/cli-moved.log"; ui_fail "Plane1 is not linked to Motor's body, or does not follow it"; }
echo "ok   Plane1 is Frame's, linked from O1 to O2, and follows Motor:1 to ($moved)"

ui_finish "UI construct occurrence test"
