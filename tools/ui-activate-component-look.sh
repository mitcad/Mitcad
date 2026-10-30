#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The question whether to activate a picked face's component (mitcad#960):
# a one-off visual check, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). A design like the case the
# question was made for: Corner brace:1 (a 60 x 40 x 5 plate) with
# M6 rod bracket:1 placed in it (a 20 x 20 x 15 block on the plate).
# Corner brace is active; Create Sketch on the block's top face asks
#   "The face belongs to M6 rod bracket:1, but Corner brace:1 is active."
# with Activate M6 rod bracket:1 (the default), Keep in Corner brace:1 and
# Don't ask again. It saves a picture of the screen with the question as
# <dir>/question.png, answers Activate, and checks that the sketch went
# into M6 rod bracket. The design is <dir>/look.mitcad, the app's log
# <dir>/look.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-activate-component-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"
# The question, which the tests turn off.
export MITCAD_ASK_ACTIVATE_COMPONENT=1

DIR=${1:?usage: tools/ui-activate-component-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$DIR/look.log" 2> /dev/null; ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Corner brace (C1, O1): Sketch1 (F1), Extrude1 (F2): the plate F2.b0.
# M6 rod bracket (C2, in Corner brace:1) placed at (10, 10, 5): Sketch2
# (F3), Extrude2 (F4): the block F4.b0.
cat > "$DIR/look.json" << EOF
[
  {"cmd": "create_component", "name": "Corner brace"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "create_component", "name": "M6 rod bracket", "transform": [[1, 0, 0, 10], [0, 1, 0, 10], [0, 0, 1, 5]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 15}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$DIR/look.json" --save "$DIR/look.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli (the design)"; }

ui_start_display
ui_start_app --open "$DIR/look.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser radio Root/Corner brace:1 at" "Corner brace in the browser"

ui_mark
ui_step "activate Corner brace (radio)"    ui_click_logged "Browser radio Root/Corner brace:1"
ui_expect_new "Activated Corner brace" "Corner brace is active"
ui_sync
ui_mark
ui_step "create sketch (search)"           ui_command "Create Sketch"
ui_expect_new "Pick face F4.b0/F4:end(" "the block's top face can be picked"
ui_step "pick the block's top face"        ui_click_pick "face F4.b0/F4:end("
ui_expect_new "Activate component asked: M6 rod bracket:1 (active Corner brace:1)" "the question"
ui_focus_dialog '^Activate Component$'
# The mouse off the window (no hover, no tooltip), the dialog drawn.
xdotool mousemove 5000 5000
ui_sync
sleep 1
ui_capture "$DIR/question.png"
echo "ok   $DIR/question.png"
ui_step "activate M6 rod bracket:1 (Enter)" ui_key Return
ui_focus_main
ui_expect_new "Activate component answer: activate M6 rod bracket:1" "answered Activate"
ui_expect_new "Activated M6 rod bracket" "M6 rod bracket is active"
ui_expect_new "Sketch Sketch3 is in M6 rod bracket" "the sketch is M6 rod bracket's"
ui_crashed && ui_fail "the application crashed"
ui_finish "Activate component question picture in $DIR"
