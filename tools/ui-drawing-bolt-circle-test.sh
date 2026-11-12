#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs core/model/src/drawing tools/cli
# A flange's bolt circle and the angle between its holes (mitcad#1693)
# through the real UI, on a plate that mitcad-cli makes: 120 x 120 x 5 with
# six holes of 10 through it on a circle of 60 about (60, 60), from 0
# degrees every 60; a drawing of it with its top view at 1:1, so that the
# plate's point (x, y) lies at (150 + x, 90 + y) on the sheet:
#   1. Center Pattern: one hole's rim; every hole of the pattern marked.
#   2. Dimension, Auto: a click on the pattern's circle between two holes
#      picks that circle ("pattern_circle"), placed: a diameter, Ø60 B.C.
#   3. Dimension, Angle: the radial lines of the holes at 60 and 120
#      degrees, outside their rims, picked as centre lines
#      ("pattern_line"), placed between them: 60°.
# tools/ui-windows-drawing-bolt-circle-test.ps1 is the Windows
# counterpart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-bolt-circle-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-bolt-circle.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE=$WORK/flange.mitcad

cat > "$WORK/flange.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 120, "height": 120},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [90, 60], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [75, 85.98076211353316], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [45, 85.98076211353316], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 60], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [45, 34.01923788646684], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [75, 34.01923788646684], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [
      {"sketch": "F3", "region": "r{c1}"}, {"sketch": "F3", "region": "r{c3}"},
      {"sketch": "F3", "region": "r{c5}"}, {"sketch": "F3", "region": "r{c7}"},
      {"sketch": "F3", "region": "r{c9}"}, {"sketch": "F3", "region": "r{c11}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [210, 150], "scale": "1:1"}
]
EOF
"$CLI" run "$WORK/flange.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# sheet_at x y: the screen position of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)).
sheet_at() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
# polar degrees radius: the sheet point of the plate's point at an angle
# and a distance from the pattern's centre (60, 60).
polar() {
  awk -v d="$1" -v r="$2" 'BEGIN { a = d * atan2(0, -1) / 180
    printf "%.2f %.2f\n", 150 + 60 + r * cos(a), 90 + 60 + r * sin(a) }'
}
polar_click() { ui_sync; xdotool mousemove $(sheet_at $(polar "$1" "$2")) click 1; ui_sync; }

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

no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "flange.mitcad" "the design opened"

echo "--- The flange's drawing"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
ui_expect_new "Drawing sheet view " "the sheet placed"
ui_expect_new "Drawing sheet drawn: 1 views" "its top view"

echo "--- 1. A centre pattern on the holes"
ui_mark
settled "Drawing tool Center Pattern"
ui_step "Center Pattern"                   ui_click_logged "Drawing tool Center Pattern"
ui_expect_new "Drawing tool: Center Pattern" "the tool is on"
ui_step "the rim of the hole at 0 degrees" polar_click 0 35
ui_expect_new '"type":"center_pattern"' "a centre pattern asked for"
ui_expect_new "1 = center_pattern" "drawn"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape

echo "--- 2. The bolt circle's diameter (Auto)"
ui_mark
settled "Drawing tool Dimension"
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
ui_step "the pattern's circle at 30 degrees" polar_click 30 30
ui_expect_new "Drawing picked circle (1)" "a circle picked"
ui_expect_new '"kind":"pattern_circle"' "the centre pattern's circle"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "where it goes, up right"          polar_click 45 70
ui_expect_new '"type":"dimension"' "a dimension asked for"
ui_expect_new "2 = Ø60 B.C." "a diameter of 60, the bolt circle's"
no_refusal

echo "--- 3. The angle between two radial lines (Angle)"
ui_mark
settled "Drawing panel drawingOption_type"
ui_step "Type: Angle"                      ui_click_logged "Drawing panel drawingOption_type"
ui_key Home Down Down Down Return
ui_expect_new "Drawing option type: angle" "an angle next"
ui_step "the radial line at 60 degrees"    polar_click 60 35.6
ui_expect_new "Drawing picked center line (1)" "a centre line picked"
ui_expect_new '"kind":"pattern_line"' "the pattern's radial line"
ui_step "the radial line at 120 degrees"   polar_click 120 35.6
ui_expect_new "Drawing picked center line (2)" "the second centre line picked"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "where it goes, between them"      polar_click 90 45
ui_expect_new '"type":"angle"' "an angle asked for"
ui_expect_new "3 = 60°" "60 degrees between the holes"
no_refusal
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "end the tool (Esc)"               ui_key Escape

ui_finish "UI drawing bolt circle test"
