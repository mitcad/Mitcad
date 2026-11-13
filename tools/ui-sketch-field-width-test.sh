#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework app/MainWindow.cpp app/commands/SketchCommands.cpp app/sketch tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# A value field grows with the expression typed into it, so that the
# whole text is visible while it is typed (mitcad#1724), on a design
# mitcad-cli makes: Sketch1 (F1) on XY with one fixed line c1 from (0, 0)
# to (40, 0).
#   1. The circle tool's Diameter field: a 30-character expression typed
#      after the centre's click widens the field from its own 104 px until
#      its text fits.
#   2. The sketch dimension editor: the same expression typed over the
#      line's length widens the editor until its text fits.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-field-width-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-field-width.XXXXXX)
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

# 30 characters, a valid expression (37) without parameter names, so no
# suggestion list opens while it is typed.
EXPRESSION="(12.5 + 7.25) * 2 - 3.75 / 1.5"
[ "${#EXPRESSION}" -eq 30 ] || ui_fail "the expression has ${#EXPRESSION} characters, expected 30"

# expect_fits prefix base what: the newest "<prefix> is W px wide: its text
# T px, room R px" since the mark shows a field wider than base whose room
# holds its whole text.
expect_fits() {
  local line width text room
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1 is " | tail -1)
  [ -n "$line" ] || ui_fail "$3 did not grow with the typed expression"
  read -r width text room <<< "$(sed -E 's/.* is ([0-9]+) px wide: its text ([0-9]+) px, room ([0-9]+) px.*/\1 \2 \3/' <<< "$line")"
  [ "$width" -gt "$2" ] || ui_fail "$3 is $width px wide, expected wider than $2 px"
  [ "$room" -ge "$text" ] || ui_fail "$3 gives its text $room px of the $text px it needs: $line"
  echo "ok   $3 is $width px wide, its whole text ($text px) visible"
}

# zoom_to_show x1 y1 x2 y2: zooms out (the wheel at the view's middle)
# until the sketch rectangle from (x1, y1) to (x2, y2) mm is in the view
# (F6 fits the 40 mm line to the whole view, so the clicks beside it would
# miss the view).
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
ui_step "show the line and room around"  zoom_to_show -40 -40 80 40

echo "--- 1. The circle's Diameter field grows with its text"
ui_step "circle tool (C)"                ui_key c
ui_step "circle: centre"                 ui_sketch_click 20 20
ui_mark
ui_step "type the expression"            xdotool type --delay 40 "$EXPRESSION"
ui_sync
typed=$(ui_typed_since "$UI_MARK" "Sketch value diameter: ")
case $typed in
  "$EXPRESSION = "*) echo "ok   the Diameter field has the whole expression" ;;
  *) ui_fail "the Diameter field has '$typed', expected '$EXPRESSION' with its value" ;;
esac
expect_fits "Sketch field diameter" 104 "the Diameter field"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "cancel the circle (Esc)"        ui_key Escape
ui_sync
ui_step "end the circle tool (Esc)"      ui_key Escape
ui_sync

echo "--- 2. The sketch dimension editor grows with its text"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the line"                  ui_sketch_click 20 0
ui_step "place its value"                ui_sketch_click 20 -10
ui_expect_new "Sketch value editor at" "the dimension editor opened"
ui_mark
ui_step "type the expression over the value" xdotool type --delay 40 "$EXPRESSION"
ui_sync
expect_fits "Sketch value editor" 130 "the dimension editor"
ui_step "close the editor (Esc)"         ui_key Escape
ui_sync

ui_finish "UI sketch field width test"
