#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/sketch app/browser
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: app/MainWindow.cpp core/model/src/features/sketch.rs
# Project Again (mitcad#1303) through the real UI, on a design mitcad-cli
# makes: a 60 x 40 block 20 high with a fillet of 5 on its vertical edge at
# (60, 0); Sketch2 (F4) on XY holds the block's bottom face as an earlier
# version recorded it (no by_geometry): 70 wide from x = -10, sharp
# corners. A line of the user's at x = 20 ends on its bottom and top lines
# (two Coincident constraints), and Extrude2 uses the region right of it.
# The face has an arc now, so the projection keeps its geometry and warns.
#   1. The timeline's tooltip of Sketch2 offers Project Again; its context
#      menu lists it, and choosing it projects the face again at once (one
#      undo step, which Ctrl+Z takes back).
#   2. Sketch2 edited: the Sketch Palette shows Project Again beside Clean
#      Up Stale Projections under the warning; its click starts the command,
#      whose preview is ok, and OK keeps it: the warning is gone, and the
#      user's line still ends on the new bottom and top lines (the
#      constraints carried over), so no degree of freedom is added.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-project-again-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-project-again.XXXXXX)
FILE=$WORK/stale.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 5}},
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p1", "type": "point", "at": [-10, 0], "fixed": true, "reference": true},
      {"id": "p2", "type": "point", "at": [60, 0], "fixed": true, "reference": true},
      {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
      {"id": "p4", "type": "point", "at": [60, 40], "fixed": true, "reference": true},
      {"id": "c5", "type": "line", "start": "p2", "end": "p4", "fixed": true, "reference": true},
      {"id": "p6", "type": "point", "at": [-10, 40], "fixed": true, "reference": true},
      {"id": "c7", "type": "line", "start": "p4", "end": "p6", "fixed": true, "reference": true},
      {"id": "c8", "type": "line", "start": "p6", "end": "p1", "fixed": true, "reference": true},
      {"id": "p9", "type": "point", "at": [20, 0]},
      {"id": "p10", "type": "point", "at": [20, 40]},
      {"id": "c11", "type": "line", "start": "p9", "end": "p10"}],
    "constraints": [
      {"id": "k12", "type": "coincident", "point": "p9", "entity": "c3"},
      {"id": "k13", "type": "coincident", "point": "p10", "entity": "c7"}],
    "projections": [{"source": "F2:start($RECT)",
                     "entities": ["p1", "p2", "c3", "p4", "c5", "p6", "c7", "c8"]}]}},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "r{c3[c11,c5],c5[c3,c7],c7[c5,c11],c11[c3,c7]}"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# dof: the degrees of freedom of Sketch2 the app logged last.
dof() { sed -n 's/^Sketch Sketch2: \([0-9]*\) DOF.*/\1/p' "$UI_LOG" | tail -1; }

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch2 at" "the timeline is logged"

echo "--- 1. The timeline offers Project Again"
ui_mark
ui_step "hover Sketch2 in the timeline"  xdotool mousemove $(ui_logged_at "Timeline Sketch2")
ui_expect_new "Project Again (right-click) projects them again" "the tooltip offers Project Again" 15
ui_mark
ui_step "right-click Sketch2"            ui_click_logged "Timeline Sketch2" 3
ui_expect_new "Context menu: " "the menu opened"
ui_step "choose Project Again"           ui_menu_choose "Project Again"
ui_expect_new "Project Again in Sketch2: " "Project Again ran at once"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Project Again in Sketch2" "one undo step takes it back"

echo "--- 2. The Sketch Palette offers Project Again under the warning"
ui_mark
ui_step "edit Sketch2 (double-click)"    ui_double_click_logged "Timeline Sketch2"
ui_expect_new "Editing sketch F4 (Sketch2)" "Sketch2 is edited" 30
ui_expect_new "Palette paletteProjectAgain at" "the palette offers Project Again"
ui_expect_new "Palette paletteCleanUpStale at" "and Clean Up Stale Projections beside it"
ui_expect_log "Sketch Palette warnings: the projection of" "the palette shows the warning"
ui_sync
before=$(dof)
ui_mark
ui_step "Project Again from the palette" ui_click_logged "Palette paletteProjectAgain"
ui_expect_new "Palette: Project Again from the warning" "the warning started the command"
ui_expect_new "Preview Project Again: ok" "the preview projects the face again"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Projected again" "kept"
ui_expect_new "Sketch Palette warnings: none" "the warning is gone"
ui_sync
now=$(dof)
[ "$now" = "$before" ] || ui_fail "the user's line is no longer held: $now DOF, before $before"
echo "ok   the user's line still ends on the new lines ($now DOF)"

ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
ui_finish "UI Project Again test"
