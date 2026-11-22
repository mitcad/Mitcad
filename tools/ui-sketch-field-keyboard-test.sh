#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework app/MainWindow.cpp app/commands/SketchCommands.cpp app/sketch tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# A dimension is typed without a click into its field (mitcad#338), on a
# design mitcad-cli makes: Sketch1 (F1) on XY with one fixed line c1 from
# (0, 0) to (40, 0).
#   1. Offset starts with the Curves selection, so the view has the
#      keyboard; once c1 is picked the Offset Position field gets it, with
#      its text selected: "12" typed and Enter (no click into the field)
#      offsets c1 by 12.
#   2. A click into the field is the user's choice: a later pick in the
#      view does not take the keyboard back from it (the typed text stays
#      where the user put it).
#   3. A drawing tool already has the keyboard in its first on-canvas
#      field: the circle's centre clicked, "30" typed and Enter (no click)
#      adds a circle of diameter 30.
#   4. Starting and cancelling Offset retires the Dimension tool's value
#      editor: a later Enter and view click cannot add a dimension.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-field-keyboard-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-field-keyboard.XXXXXX)
FILE=$WORK/line.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [40, 0]},
  {"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["c1"]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# zoom_to_show x1 y1 x2 y2: zooms out (the wheel at the view's middle)
# until the sketch rectangle from (x1, y1) to (x2, y2) mm is in the view.
zoom_to_show() {
  local area vx vy vw vh ax ay bx by
  for _ in $(seq 1 20); do
    area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
    read -r _ _ vx vy vw vh <<< "$area"
    read -r ax ay <<< "$(ui_sketch_at "$1" "$2")"
    read -r bx by <<< "$(ui_sketch_at "$3" "$4")"
    if [ "$ax" -gt $((X + vx + 20)) ] && [ "$bx" -lt $((X + vx + vw - 20)) ] &&
      [ "$by" -gt $((Y + vy + 20)) ] && [ "$ay" -lt $((Y + vy + vh - 20)) ]; then
      return 0
    fi
    xdotool mousemove $(ui_view_at 50 50) click 5
    ui_sync
  done
  ui_fail "the view did not zoom out to show ($1, $2) to ($3, $4)"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch1 at" "Sketch1 in the timeline"
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync
ui_step "show the line and room around"  zoom_to_show -20 -30 60 30

echo "--- Dimension editor ends when a panel command starts (mitcad#351)"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the line"                  ui_sketch_click 20 0
ui_step "place its value"                ui_sketch_click 20 10
ui_expect_new "Sketch value editor at" "the dimension editor opened"
ui_step "open MODIFY from the ribbon"    ui_click_logged "Ribbon SKETCH/MODIFY"
# Offset is the ninth entry of MODIFY (after Extend to Meet, mitcad#2178).
ui_step "choose Offset"                  ui_key Down Down Down Down Down Down Down Down Down Return
ui_expect_new "Command Offset started" "Offset started from the ribbon"
ui_expect_new "Sketch value editor closed" "the retired tool's editor closed"
ui_step "cancel Offset (Esc)"            ui_key Escape
ui_expect_new "Command Offset cancelled" "Offset cancelled"
ui_step "Enter after the tool ended"     ui_key Return
ui_step "click after the tool ended"     ui_sketch_click 25 15
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -Eq "Added (driven )?dimension"; then
  ui_fail "a retired Dimension tool added a dimension"
fi
echo "ok   no dimension accepted after its tool ended"

echo "--- 1. Offset: pick the curve, type the distance with keys only"
ui_mark
ui_step "offset (O)"                     ui_key o
ui_expect_new "Offset Curves: nothing" "Offset waits for its curves"
ui_mark
ui_step "pick c1"                        ui_sketch_click 20 0
ui_expect_new "Offset Curves: 1 sketch curve" "the curve went to the offset"
ui_step "type 12 (no click into the field)" xdotool type --delay 40 "12"
ui_sync
typed=$(ui_typed_since "$UI_MARK" "Panel field distance: ")
[ "$typed" = "12" ] || ui_fail "the Offset Position field has '$typed', expected '12' (the default is replaced)"
echo "ok   the typed number replaced the default"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Offset 1 curve(s) by 12" "offset by the typed distance"
ui_mark
ui_step "undo the offset (Ctrl+Z)"       ui_key ctrl+z
ui_expect_new "Undo: " "the offset undone"

echo "--- 2. A field the user clicked keeps the keyboard"
ui_mark
ui_step "offset (O)"                     ui_key o
ui_step "pick c1"                        ui_sketch_click 20 0
ui_expect_new "Offset Curves: 1 sketch curve" "the curve went to the offset"
ui_mark
ui_step "click into the field"           ui_click_logged "Panel Offset input distance"
ui_sync
ui_step "type 7"                         xdotool type --delay 40 "7"
ui_sync
typed=$(ui_typed_since "$UI_MARK" "Panel field distance: ")
case $typed in
  *7) echo "ok   typing into the clicked field: $typed" ;;
  *) ui_fail "the clicked field has '$typed', expected the typed 7" ;;
esac
ui_step "cancel (Esc)"                   ui_key Escape
ui_step "cancel the command (Esc)"       ui_key Escape

echo "--- 3. Circle: the diameter typed without a click"
ui_mark
ui_step "circle tool (C)"                ui_key c
ui_step "circle: centre"                 ui_sketch_click 20 -15
ui_step "type 30"                        xdotool type --delay 40 "30"
ui_step "Enter"                          ui_key Return
ui_expect_new "Added circle, diameter 30 mm at (20, -15)" "the circle has the typed diameter"
ui_step "end the tool (Esc)"             ui_key Escape

ui_finish "UI sketch field keyboard test"
