#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser app/framework/FloatingLayout.cpp app/MainWindow.cpp tools/cli
# The drawing workspace's panel can be widened (mitcad#1692, GitHub #266)
# through the real UI, on Plate (a 40 x 30 x 5 body) placed twice, with a
# drawing whose base view draws both, that mitcad-cli makes (as
# tools/ui-windows-drawing-panel-width-test.ps1 does on Windows):
#   - Floating, a fresh configuration: the panel is 230 px wide; the base
#     view's Shows spans it under its label, and Update from Model's whole
#     text fits its button.
#   - The panel's edge dragged toward the sheet: the panel and Shows are
#     wider by the same amount, and the width is remembered.
#   - Close Drawing, the drawing opened again: the panel keeps its width.
#   - Docked, at the next start: the panel has the width again; its edge
#     dragged wider still is remembered too.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-panel-width-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d "$HOME/mitcad-ui-drawing-panel-width.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/plates.mitcad"

cat > "$WORK/plates.json" << EOF
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "copy_occurrence", "occurrence": "Plate:1", "transform": {"translation": [50, 0, 0]}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front", "at": [150, 150], "scale": "1:1"}
]
EOF
"$CLI" run "$WORK/plates.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
echo "ok   a design with a drawing made"

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait.
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

# last_width "Drawing panel <what>": the number after "<what> width " in
# the log's last such line since the mark.
last_width() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -oE "^$1 width [0-9]+" | tail -1 | awk '{ print $NF }'
}

# wait_width "Drawing panel <what>" width why: until the last such line
# since the mark has that width.
wait_width() {
  local got=""
  for _ in $(seq 1 50); do
    got=$(last_width "$1")
    [ "$got" = "$2" ] && { echo "ok   $3 ($1 width $2)"; return 0; }
    ui_sync
    sleep 0.2
  done
  ui_fail "$3: $1 is ${got:-?} px wide, not $2"
}

# select_view: the base view selected, its panel laid out (Shows and Update
# from Model in it).
select_view() {
  ui_mark
  ui_step "select the front view"          ui_click_logged "Drawing view 1"
  ui_expect_new "Drawing view 1 selected" "the front view selected"
  ui_expect_new "Drawing panel drawingViewShows at" "its Shows in the panel"
  ui_expect_new "Drawing panel drawingViewUpdate at" "Update from Model in the panel"
  settled "Drawing panel drawingViewUpdate"
}

# remembered: the width of the "Drawing panel width N remembered" line
# since the mark (the edge let go), waited for; empty when none came.
remembered() {
  local got=""
  for _ in $(seq 1 50); do
    got=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -oE "^Drawing panel width [0-9]+ remembered$" | tail -1 | awk '{ print $4 }')
    [ -n "$got" ] && break
    sleep 0.2
  done
  echo "$got"
}

# button_fits: Update from Model's text fits its button.
button_fits() {
  local line width needs
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "^Drawing panel drawingViewUpdate width [0-9]+, its text needs [0-9]+$" | tail -1)
  [ -n "$line" ] || ui_fail "the button's width was not logged"
  width=$(awk '{ print $5 }' <<< "${line//,/}")
  needs=$(awk '{ print $NF }' <<< "$line")
  [ "$needs" -le "$width" ] || ui_fail "Update from Model needs $needs px, its button is $width px: $1"
  echo "ok   Update from Model's text fits ($needs of $width px): $1"
}

close_drawing() {
  ui_mark
  settled "Drawing Close Drawing"
  ui_step "Close Drawing"                  ui_click_logged "Drawing Close Drawing"
  ui_expect_new "Drawing closed" "the 3D view again"
}

open_drawing() {
  ui_mark
  ui_step "open Drawing1 (double-click)"   ui_double_click_logged "Browser Root/Drawings/Drawing1"
  ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
  ui_expect_new "Drawing sheet drawn: " "its sheet drawn"
}

ui_start_display

echo "--- Floating: the panel at its narrowest"
ui_start_app --chrome=floating --open "$FILE" --no-native-dialogs
ui_expect_log "plates.mitcad" "the design opened"
open_drawing
select_view
wait_width "Drawing panel" 230 "the panel at its narrowest at first"
shows=$(last_width "Drawing panel drawingViewShows")
[ -n "$shows" ] || ui_fail "the Shows list's width was not logged"
[ "$shows" -ge 200 ] || ui_fail "Shows is $shows px wide in a 230 px panel: not across the panel"
echo "ok   Shows spans the panel ($shows px)"
button_fits "at the narrowest panel"

echo "--- The panel's edge dragged toward the sheet"
ui_mark
ui_step "drag the panel's edge to the left" ui_drag_from "Drawing panel edge" $((-150 * UI_SCALE)) 0
wide=$(remembered)
[ -n "$wide" ] && [ "$wide" -ge 340 ] || ui_fail "the panel is ${wide:-?} px wide after the drag"
echo "ok   the panel is $wide px wide"
wait_width "Drawing panel" "$wide" "the panel laid out at its new width"
wait_width "Drawing panel drawingViewShows" $((shows + wide - 230)) "Shows wider by the same amount"
button_fits "in the wider panel"

echo "--- The drawing opened again: the width kept"
close_drawing
open_drawing
select_view
wait_width "Drawing panel" "$wide" "the panel keeps its width"
wait_width "Drawing panel drawingViewShows" $((shows + wide - 230)) "Shows keeps its width"

echo "--- Docked, at the next start: the width again, and wider still"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
UI_MARK=0 # a new log
ui_expect_log "plates.mitcad" "the design opened"
open_drawing
ui_expect_log "Drawing panel width $wide (remembered)" "the width read from the settings"
select_view
wait_width "Drawing panel" "$wide" "the panel has its width again"
wait_width "Drawing panel drawingViewShows" $((shows + wide - 230)) "Shows has its width again"
ui_mark
ui_step "drag the panel's edge to the left" ui_drag_from "Drawing panel edge" $((-60 * UI_SCALE)) 0
wider=$(remembered)
[ -n "$wider" ] && [ "$wider" -ge $((wide + 40)) ] || ui_fail "the docked panel is ${wider:-?} px wide (was $wide)"
echo "ok   the docked panel is $wider px wide (was $wide)"
wait_width "Drawing panel drawingViewShows" $((shows + wider - 230)) "Shows wider by the same amount, docked"

grep -q "Drawing command refused" "$UI_LOG" && ui_fail "a drawing command was refused"
ui_finish "UI drawing panel width test"
