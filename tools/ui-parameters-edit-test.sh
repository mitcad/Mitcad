#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowParameterEdits.cpp app/MainWindow.cpp app/framework/CommandSessionParameters.cpp
# check-all sources: app/framework/CommandSessionVisibility.cpp app/framework/ParameterEdits.cpp
# check-all sources: app/framework/CommandPanelHeader.cpp app/framework/CommandSearch.cpp app/browser tools/cli
# Edit Parameters while a sketch or a feature is being created or edited
# (mitcad#564), through the real UI:
#   - A design (mitcad-cli): Sketch1 (F1), a 60 x 40 rectangle whose width
#     is the named dimension `width`, extruded by Extrude1 (F2) the user
#     parameter `depth` (20 mm) high; Sketch2 (F3), a free 20 x 20 square.
#   - Editing Sketch1: the command search runs SKETCH > MODIFY's Change
#     Parameters; width = 50 mm reshapes the sketch at once, 30 deg is
#     refused and the sketch keeps 50 mm; Undo and Redo in the sketch take
#     the change back and again; Finish Sketch makes it part of the edit's
#     one undo step (Body1 50 x 40 x 20).
#   - Editing Extrude1: the panel's Edit Parameters button opens the
#     table beside the panel; Extrude1's own d3 is left to the panel;
#     depth = 30 mm evaluates the panel's `depth` again and previews; the
#     panel then takes `depth + 5 mm`; OK: the parameter change and the
#     edit are two undo steps (Undo, Undo, Redo, Redo).
#   - Editing Extrude1 again, depth = 10 mm, Cancel: the change stays (one
#     undo step), the edit does not.
#   - A new Extrude of Sketch2's square at `depth`: the table renames depth
#     to dp, the panel's expression follows; OK adds it 10 mm high.
#   - The saved design: Body1 50 x 40 x 15, Body2 20 x 20 x 10.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameters-edit-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-parameters-edit.XXXXXX)
FILE=$WORK/parameters.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [100, 0], "width": 20, "height": 20},
  {"cmd": "add_parameter", "name": "depth", "expression": "20 mm", "unit": "mm"},
  {"cmd": "set_parameter", "name": "d3", "expression": "depth"},
  {"cmd": "rename_parameter", "name": "d1", "new_name": "width"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_change before after description: the last change of Body1's
# volume logged since the mark.
expect_change() {
  local line was now
  for _ in $(seq 1 75); do
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n 's/.*Body Body1 (F2\.b0): volume \([0-9.]*\) -> \([0-9.]*\) mm3.*/\1 \2/p' | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  [ -n "$line" ] || ui_fail "$3: no volume change logged"
  read -r was now <<< "$line"
  awk -v w="$was" -v n="$now" -v a="$1" -v b="$2" \
    'BEGIN { exit !((w - a) ^ 2 < (1e-6 * a) ^ 2 + 1e-4 && (n - b) ^ 2 < (1e-6 * b) ^ 2 + 1e-4) }' ||
    ui_fail "$3: volume $was -> $now mm3, expected $1 -> $2"
  echo "ok   $3: $was -> $now mm3"
}

# edit_cell "name column" text: double-click a cell of the table, replace
# its text, Enter.
edit_cell() {
  ui_focus_dialog '^Parameters$'
  ui_double_click_logged "Parameters $1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "$2"
  ui_key Return
}

# close_table "what": OK of the table, back to what stayed open.
close_table() {
  ui_mark
  ui_focus_dialog '^Parameters$'
  ui_step "OK (the table)"               ui_click_logged "Parameters OK"
  ui_expect_new "Parameters closed: back to $1" "back to $1"
  ui_focus_main
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"

echo "--- Editing Sketch1: SKETCH > MODIFY > Edit Parameters"
ui_mark
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 is edited" 30
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Command search chose sketch.parameters" "the search runs the SKETCH entry"
ui_expect_new "Parameters dialog opened" "the table opened"
ui_expect_new "Parameters during sketch Sketch1" "beside the sketch"
ui_expect_log "Parameters width expression at" "the named dimension's cells are logged"
ui_mark
ui_step "width = 50 mm"                  edit_cell "width expression" "50 mm"
ui_expect_new "Parameter width expression: 50 mm = 50 mm" "width changed"
ui_expect_new "Sketch Sketch1 dimensions after the parameter change: " "the sketch follows at once"
dimensions=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Sketch Sketch1 dimensions after the parameter change: " | tail -1)
case "$dimensions" in
  *"width 50.000"*"d2 40.000"* | *"d2 40.000"*"width 50.000"*) echo "ok   the sketch is 50 x 40" ;;
  *) ui_fail "the sketch's dimensions: $dimensions" ;;
esac
ui_mark
ui_step "width = 30 deg (refused)"       edit_cell "width expression" "30 deg"
ui_expect_new "Parameter width expression: 30 deg refused: " "an angle refused for a length"
sleep 0.5
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Sketch Sketch1 dimensions after the parameter change" &&
  ui_fail "a refused change reshaped the sketch"
echo "ok   the sketch keeps the last valid value"
[ -n "$SHOT" ] && ui_capture "$SHOT"
close_table "Sketch1"
ui_mark
ui_step "undo (Ctrl+Z) in the sketch"    ui_key ctrl+z
ui_expect_new "Undo: Change width" "the change is an undo step of the sketch"
ui_mark
ui_step "redo (Ctrl+Y) in the sketch"    ui_key ctrl+y
ui_expect_new "Redo: Change width" "and again"
ui_mark
ui_step "Finish Sketch (search)"         ui_command "Finish Sketch"
ui_expect_new "Sketch finished" "the sketch finished"
ui_expect_volume "Body Body1 (F2.b0): volume \([0-9.]*\) mm3" 40000 "Body1 50 x 40 x 20"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Edit Sketch1" "the change is part of the sketch's edit"
expect_change 40000 48000 "Body1 60 x 40 x 20 again"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Edit Sketch1" "the edit again"
expect_change 48000 40000 "Body1 50 x 40 x 20"

echo "--- Editing Extrude1: the panel's button, d3 left to the panel, depth = 30 mm, OK"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "Extrude1 opens for editing" 30
ui_expect_new "Extrude Distance: depth = 20 mm" "its distance is depth"
ui_expect_new "Panel Extrude parameters at" "the panel's Edit Parameters button is logged"
ui_mark
ui_step "Edit Parameters (the panel)"  ui_click_logged "Panel Extrude parameters"
ui_expect_new "Parameters dialog opened" "the table opened beside the panel"
ui_expect_new "Parameters during command Extrude editing Extrude1" "during the edit"
ui_sync
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "d3's expression (held)"         ui_double_click_logged "Parameters d3 expression"
ui_expect_new "Parameter d3 is held by the open panel of Extrude1" "Extrude1's own value is left to its panel"
ui_mark
ui_step "depth = 30 mm"                  edit_cell "depth expression" "30 mm"
ui_expect_new "Parameter depth expression: 30 mm = 30 mm" "depth changed"
ui_expect_new "Extrude: parameter depth changed; the preview follows" "the panel follows"
ui_expect_new "Extrude Distance: depth = 30 mm" "the distance evaluated again"
ui_expect_new "Preview Extrude: ok" "previewed again" 30
close_table "Extrude"
ui_mark
ui_step "distance depth + 5 mm"          ui_type_in "Panel Extrude input distance" "depth + 5 mm"
ui_expect_new "Extrude Distance: depth + 5 mm = 35 mm" "the panel takes an expression of it"
ui_expect_new "Preview Extrude: ok" "previewed" 30
ui_mark
ui_step "OK (the panel)"                 ui_click_logged "Panel Extrude OK"
ui_expect_new "Command Extrude: 1 parameter change(s) kept" "the parameter change kept before the edit"
ui_expect_new "Edited F2" "Extrude1 edited" 30
expect_change 60000 70000 "the edit after depth = 30 mm: 50 x 40 x 35"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Edit Extrude1" "the edit is one undo step"
expect_change 70000 60000 "50 x 40 x 30"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Change depth" "the parameter change is the step before it"
expect_change 60000 40000 "50 x 40 x 20"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Change depth" "the parameter change again"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Edit Extrude1" "the edit again"
expect_change 60000 70000 "50 x 40 x 35"

echo "--- Editing Extrude1 again: depth = 10 mm, Cancel"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "Extrude1 opens for editing" 30
ui_expect_new "Extrude Distance: depth + 5 mm = 35 mm" "its distance as edited"
ui_expect_new "Panel Extrude parameters at" "the panel's Edit Parameters button is logged"
ui_mark
ui_step "Edit Parameters (the panel)"  ui_click_logged "Panel Extrude parameters"
ui_expect_new "Parameters during command Extrude editing Extrude1" "the table beside the edit"
ui_sync
ui_mark
ui_step "depth = 10 mm"                  edit_cell "depth expression" "10 mm"
ui_expect_new "Parameter depth expression: 10 mm = 10 mm" "depth changed"
ui_expect_new "Extrude Distance: depth + 5 mm = 15 mm" "the panel follows"
close_table "Extrude"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude: 1 parameter change(s) kept" "the parameter change kept"
ui_expect_new "Command Extrude cancelled" "the edit cancelled"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Change depth" "the change stayed as its own undo step"
expect_change 30000 70000 "Body1 was 50 x 40 x 15 after the cancel"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Change depth" "the change again"
expect_change 70000 30000 "50 x 40 x 15"

echo "--- A new Extrude: depth renamed while its panel uses it"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Command Extrude started" "Extrude started"
ui_expect_new "Extrude Profiles: 1 profile [profile $RECT of F3]" "it takes Sketch2's square"
ui_expect_new "Panel Extrude parameters at" "the panel's Edit Parameters button is logged"
ui_mark
ui_step "distance depth"                 ui_type_in "Panel Extrude input distance" "depth"
ui_expect_new "Extrude Distance: depth = 10 mm" "the distance is depth"
ui_mark
ui_step "Edit Parameters (the panel)"  ui_click_logged "Panel Extrude parameters"
ui_expect_new "Parameters during command Extrude" "the table beside the new feature"
ui_sync
ui_mark
ui_step "rename depth to dp"             edit_cell "depth name" "dp"
ui_expect_new "Parameter depth name: dp = 10 mm" "renamed"
ui_expect_new "Extrude distance: depth becomes dp (depth renamed)" "the panel's expression follows the new name"
ui_expect_new "Extrude Distance: dp = 10 mm" "and is evaluated"
close_table "Extrude"
ui_mark
ui_step "OK (the panel)"                 ui_click_logged "Panel Extrude OK"
ui_expect_new "Added Extrude as F4" "the extrusion added" 30
ui_expect_new "New body Body2 (F4.b0): volume 4000.000 mm3" "20 x 20 x 10"

echo "--- The saved design"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
cat > "$WORK/check.json" << 'EOF'
[
  {"expect": {"body": "Body1", "volume": 30000, "tolerance": 1e-6}},
  {"expect": {"body": "Body2", "volume": 4000, "tolerance": 1e-6}}
]
EOF
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 || {
  cat "$WORK/check.log"
  ui_fail "the saved design"
}
echo "ok   Body1 50 x 40 x 15, Body2 20 x 20 x 10"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Add Extrude2" "the new extrusion is one undo step"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Rename depth to dp" "the rename is the step before it"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI parameters edit test"
