#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The timeline (U3) through the real UI, with the volumes the model reports:
#   - A failed feature: its tooltip, the status bar's summary that goes to
#     it, and Delete without dependents.
#   - Suppress and unsuppress; the history marker dragged back (one undo
#     step), the playback buttons, Roll History Marker Here.
#   - Reorder by dragging: a plane goes first; a fillet cannot go before
#     its body, and nothing changes.
#   - Rename in place, edit by double-click, a click selecting the
#     feature's body, Find in Browser, Delete with its dependents (the
#     dialog: Cancel, then Delete All), undo.
#   - A warning (P9): a full round built 0.1 % smaller is yellow, its
#     tooltip and its panel's preview say why.
#   - Timeline groups (P9): a Shift+click run grouped, folded into one
#     cell and opened again, renamed, saved.
#   - The faces features made (mitcad#28): a click on a fillet highlights
#     its rounded face, a Shift+click run the faces of each feature in it;
#     Esc and a click in the view clear it; a chamfer after the marker or
#     suppressed highlights nothing.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timeline-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-timeline.XXXXXX)
FILE=$WORK/block.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# expect_volume "log regex with one number" expected description (lines
# after the mark only)
expect_volume() {
  local volume
  volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n "s/.*$1.*/\\1/p" | tail -1)
  [ -n "$volume" ] || ui_fail "$3: no volume logged"
  awk -v v="$volume" -v e="$2" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }' ||
    ui_fail "$3: volume $volume mm3, expected $2"
  echo "ok   $3: $volume mm3"
}

# expect_last "prefix" "rest" description: the last line logged since the
# mark that has the prefix ends with exactly that rest after it.
expect_last() {
  local line
  for _ in $(seq 1 30); do
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1)
    [ "${line#*"$1"}" = "$2" ] && { echo "ok   $3"; return; }
    sleep 0.2
  done
  ui_fail "$3: last '$1' line: '$line', expected '$1$2'"
}

# The middle between two logged places, moved dx pixels: where to drop.
between() {
  local ax ay bx by
  read -r ax ay <<< "$(ui_logged_at "$1")"
  read -r bx by <<< "$(ui_logged_at "$2")"
  echo $(((ax + bx) / 2 + ${3:-0})) $(((ay + by) / 2))
}

# drag_to "from text" x y: a left drag from a logged place to a point.
drag_to() {
  local fx fy i
  read -r fx fy <<< "$(ui_logged_at "$1")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4 5 6 7 8; do
    xdotool mousemove $((fx + ($2 - fx) * i / 8)) $((fy + ($3 - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
}

# Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1, 6000 mm3; Plane1 (F3);
# Fillet1 (F4) r 1 on a vertical edge: (1 - pi/4) x 10 mm3 less; Fillet2
# (F5) r 50 fails.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 15}}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 1}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c3[c2,c4])|F2:side(c4[c3,c1])}"], "radius": 50}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
FILLETED=$(awk 'BEGIN { printf "%.6f", 6000 - (1 - 3.14159265358979 / 4) * 10 }')

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Fillet2 at" "the timeline is logged"

echo "--- A failed feature"
ui_expect_log "Failed features: Fillet2: " "Fillet2 failed"
ui_step "hover over Fillet2"             xdotool mousemove $(ui_logged_at "Timeline Fillet2")
ui_expect_log "Timeline tooltip: Fillet2: " "its tooltip has the error"
ui_mark
ui_step "click the failures summary"     ui_click_logged "Failures button"
ui_expect_new "Revealed Fillet2" "the summary went to Fillet2"
ui_mark
ui_step "right-click Fillet2"            ui_click_logged "Timeline Fillet2" 3
ui_step "choose Delete"                  ui_menu_choose "Delete"
ui_expect_new "Deleted Fillet2" "deleted without asking: nothing uses it"
ui_expect_new "Failed features: none" "nothing fails"

echo "--- Suppress"
ui_mark
ui_step "right-click Fillet1"            ui_click_logged "Timeline Fillet1" 3
ui_step "choose Suppress Features"       ui_menu_choose "Suppress Features"
ui_expect_new "Suppressed Fillet1" "suppressed"
expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 6000 "the fillet is gone"
ui_mark
ui_step "right-click Fillet1"            ui_click_logged "Timeline Fillet1" 3
ui_step "choose Unsuppress Features"     ui_menu_choose "Unsuppress Features"
ui_expect_new "Unsuppressed Fillet1" "unsuppressed"
expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" "$FILLETED" "the fillet is back"

echo "--- The history marker"
ui_mark
read -r mx my <<< "$(between "Timeline Extrude1" "Timeline Plane1")"
ui_step "drag the marker after Extrude1" drag_to "Timeline marker" "$mx" "$my"
ui_expect_new "Marker at 2 of 4" "the marker after Extrude1"
expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 6000 "rolled back before the fillet"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Move Timeline Marker" "the drag is one undo step"
ui_mark
ui_step "undo again (Ctrl+Z)"            ui_key ctrl+z
ui_expect_new "Undo: Unsuppress Fillet1" "the step before the drag"
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_mark
ui_step "step back"                      ui_click_logged "Timeline button back"
ui_expect_new "Marker at 3 of 4" "one step back"
ui_mark
ui_step "move to the end"                ui_click_logged "Timeline button end"
ui_expect_new "Marker at 4 of 4" "at the end"
ui_mark
ui_step "right-click Extrude1"           ui_click_logged "Timeline Extrude1" 3
ui_step "Roll History Marker Here"       ui_menu_choose "Roll History Marker Here"
ui_expect_new "Marker at 2 of 4" "rolled to Extrude1"
ui_step "move to the end"                ui_click_logged "Timeline button end"

echo "--- Reorder by dragging"
ui_mark
read -r gx gy <<< "$(ui_logged_at "Timeline Sketch1")"
ui_step "drag Plane1 before Sketch1"     drag_to "Timeline Plane1" $((gx - 18)) "$gy"
ui_expect_new "Moved Plane1 to 0 in the timeline" "Plane1 first"
ui_expect_new "Timeline Plane1 at" "the timeline redrawn"
ui_mark
read -r gx gy <<< "$(ui_logged_at "Timeline Plane1")"
ui_step "drag Fillet1 to the start"      drag_to "Timeline Fillet1" $((gx - 18)) "$gy"
ui_expect_new "Move of Fillet1 refused: Fillet1 cannot move there" "the fillet cannot go before its body"
grep -q "Moved Fillet1" "$UI_LOG" && ui_fail "Fillet1 moved"
echo "ok   nothing moved"

echo "--- Rename, select, edit"
ui_mark
ui_step "right-click Plane1"             ui_click_logged "Timeline Plane1" 3
ui_step "choose Rename"                  ui_menu_choose "Rename"
ui_expect_new "Renaming Plane1 in the timeline" "renaming in place"
ui_step "type Datum, Enter"              xdotool type --delay 40 "Datum"
ui_key Return
ui_expect_new "Renamed Plane1 to Datum" "Plane1 is Datum"
ui_mark
ui_step "click Extrude1"                 ui_click_logged "Timeline Extrude1"
ui_expect_new "Selected: 1 body [body F2.b0]" "its body selected in the view"
ui_expect_new "Browser selected: Root/Bodies/Body1" "and in the browser"
ui_mark
ui_step "right-click Datum"              ui_click_logged "Timeline Datum" 3
ui_step "choose Find in Browser"         ui_menu_choose "Find in Browser"
ui_expect_new "Browser selected: Root/Construction/Datum" "Find in Browser"
ui_mark
ui_step "double-click Extrude1"          ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "editing Extrude1"
ui_step "type 20"                        xdotool type --delay 40 "20"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F2" "edited"
expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk -v f="$FILLETED" 'BEGIN { printf "%.6f", 2 * f }')" "twice as high"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Delete with dependents"
ui_mark
ui_step "right-click Sketch1"            ui_click_logged "Timeline Sketch1" 3
ui_step "choose Delete"                  ui_menu_choose "Delete"
ui_expect_new "Delete Sketch1: also Extrude1, Fillet1?" "the dialog lists the dependents"
ui_focus_dialog '^Delete$'
ui_step "cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Delete Sketch1 cancelled" "cancelled"
ui_mark
ui_step "right-click Sketch1"            ui_click_logged "Timeline Sketch1" 3
ui_step "choose Delete"                  ui_menu_choose "Delete"
ui_focus_dialog '^Delete$'
ui_step "Delete All (Enter)"             ui_key Return
ui_focus_main
ui_expect_new "Deleted Sketch1, Extrude1, Fillet1" "deleted with its dependents"
ui_expect_new "Removed body Body1 (F2.b0)" "the body is gone"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Delete Sketch1" "one undo step"
ui_expect_new "Timeline Fillet1 at" "the features are back"
grep -q "Recompute failed: Fillet1" "$UI_LOG" && ui_fail "Fillet1 failed"
grep -q "Feature warnings: [^n]" "$UI_LOG" && ui_fail "the block's features have warnings"

echo "--- Timeline groups (P9)"
ui_mark
ui_step "click Sketch1"                  ui_click_logged "Timeline Sketch1"
ui_step "Shift+click Fillet1"            xdotool keydown shift mousemove $(ui_logged_at "Timeline Fillet1") click 1 keyup shift
ui_expect_new "Timeline selected: F1, F2, F4" "a run of three features selected"
ui_mark
ui_step "right-click Extrude1"           ui_click_logged "Timeline Extrude1" 3
ui_step "choose Create Group"            ui_menu_choose "Create Group"
ui_expect_new "Grouped Sketch1, Extrude1, Fillet1 as Group1" "grouped"
ui_expect_new "Timeline group Group1 at" "the group's band"
ui_mark
ui_step "click the band"                 ui_click_logged "Timeline group Group1"
ui_expect_new "Timeline group Group1 collapsed" "folded into one cell"
ui_expect_new "Timeline group Group1 at" "the folded cell"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Timeline Extrude1 at" && ui_fail "Extrude1 still shown"
echo "ok   its features are not shown"
ui_mark
ui_step "double-click the folder"        ui_double_click_logged "Timeline group Group1"
ui_expect_new "Timeline group Group1 expanded" "open again"
ui_expect_new "Timeline Extrude1 at" "its features are back"
ui_mark
ui_step "right-click the band"           ui_click_logged "Timeline group Group1" 3
ui_step "choose Rename Group"            ui_menu_choose "Rename Group"
ui_focus_dialog '^Rename Group$'
ui_step "type Body, Enter"               ui_key ctrl+a
xdotool type --delay 40 "Body"
ui_key Return
ui_focus_main
ui_expect_new "Renamed group Group1 to Body" "renamed"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
groups=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["groups"])' "$FILE")
[ "$groups" = "[{'name': 'Body', 'features': ['F1', 'F2', 'F4']}]" ] || ui_fail "groups in the file: $groups"
echo "ok   the group is saved"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Rename Group1 to Body" "the rename is an undo step"
ui_stop_app

echo "--- A warning (P9)"
# A rib 2 mm wide with a full round on top: OCCT cannot take the top face
# away, so the fillet is built 0.1 % smaller and succeeds with a warning
# (tools/cli/tests/p9_warnings.json).
cat > "$WORK/rib.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 2, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})|F2:side(c2[c1,c3])}",
              "E{F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})|F2:side(c4[c3,c1])}"], "radius": 1}}
]
EOF
"$CLI" run "$WORK/rib.json" --save "$WORK/rib.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$WORK/rib.mitcad"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Feature warnings: Fillet1: the fillet is built 0.1 % smaller" "Fillet1 succeeded with a warning"
grep -q "Failed features: Fillet1" "$UI_LOG" && ui_fail "Fillet1 failed"
ui_expect_log "Timeline Fillet1 at" "the timeline is logged"
ui_step "hover over Fillet1"             xdotool mousemove $(ui_logged_at "Timeline Fillet1")
ui_expect_log "Timeline tooltip: Fillet1: the fillet is built 0.1 % smaller" "its tooltip has the warning"
ui_mark
ui_step "double-click Fillet1"           ui_double_click_logged "Timeline Fillet1"
ui_expect_new "Editing F3 with Fillet" "editing Fillet1"
ui_expect_new "Preview Fillet: ok, warning: the fillet is built 0.1 % smaller" "the panel tells" 30
ui_step "cancel (Esc)"                   ui_key Escape
ui_stop_app

echo "--- The faces features made (mitcad#28)"
# Sketch1 (F1), Extrude1 (F2): the block; Fillet1 (F3) rounds one vertical
# edge, Chamfer1 (F4) bevels another: one face each.
cat > "$WORK/faces.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 2}},
  {"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0",
    "edges": ["E{F2:side(c3[c2,c4])|F2:side(c4[c3,c1])}"],
    "size": {"type": "equal_distance", "distance": 2}}}
]
EOF
"$CLI" run "$WORK/faces.json" --save "$WORK/faces.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$WORK/faces.mitcad"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Chamfer1 at" "the timeline is logged"
ui_mark
ui_step "click Fillet1"                  ui_click_logged "Timeline Fillet1"
expect_last "Selected: " "1 feature [feature F3]" "the fillet is selected"
expect_last "Feature faces highlighted: " "F3 1" "its rounded face is highlighted"
ui_mark
ui_step "Esc"                            ui_key Escape
expect_last "Selected: " "nothing" "Esc clears the selection"
expect_last "Feature faces highlighted: " "none" "and the highlight"
ui_mark
ui_step "click Extrude1"                 ui_click_logged "Timeline Extrude1"
expect_last "Selected: " "1 body [body F2.b0]" "Extrude1 selects its body, as before"
ui_mark
ui_step "Shift+click Chamfer1"           xdotool keydown shift mousemove $(ui_logged_at "Timeline Chamfer1") click 1 keyup shift
ui_expect_new "Timeline selected: F2, F3, F4" "a run of three features"
expect_last "Selected: " "1 body, 2 features [body F2.b0; feature F3; feature F4]" "each feature of the run selected"
expect_last "Feature faces highlighted: " "F3 1, F4 1" "the fillet's and the chamfer's faces highlighted"
ui_mark
ui_step "click beside the block"         ui_view_click 4 50
expect_last "Selected: " "nothing" "a click in the view clears it"
expect_last "Feature faces highlighted: " "none" "and the highlight"
ui_mark
ui_step "right-click Fillet1"            ui_click_logged "Timeline Fillet1" 3
ui_step "Roll History Marker Here"       ui_menu_choose "Roll History Marker Here"
ui_expect_new "Marker at 3 of 4" "the marker before Chamfer1"
ui_mark
ui_step "click Extrude1"                 ui_click_logged "Timeline Extrude1"
ui_step "Shift+click Chamfer1"           xdotool keydown shift mousemove $(ui_logged_at "Timeline Chamfer1") click 1 keyup shift
ui_expect_new "Timeline selected: F2, F3, F4" "the same run"
expect_last "Selected: " "1 body, 1 feature [body F2.b0; feature F3]" "the rolled-back chamfer left out"
expect_last "Feature faces highlighted: " "F3 1" "only the fillet's face highlighted"
ui_step "move to the end"                ui_click_logged "Timeline button end"
ui_mark
ui_step "right-click Chamfer1"           ui_click_logged "Timeline Chamfer1" 3
ui_step "choose Suppress Features"       ui_menu_choose "Suppress Features"
ui_expect_new "Suppressed Chamfer1" "Chamfer1 suppressed"
ui_mark
ui_step "click Chamfer1"                 ui_click_logged "Timeline Chamfer1"
expect_last "Selected: " "nothing" "the suppressed chamfer selects nothing"
expect_last "Feature faces highlighted: " "none" "and highlights nothing"
grep -q "Recompute failed\|Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"
echo "ok   no error"

ui_finish "UI timeline test"
