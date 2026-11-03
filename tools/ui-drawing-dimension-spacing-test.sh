#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-drawing/src/spacing.rs core/model-drawing/src/annotate.rs
# check-all sources: core/model-api/src/drawing_spacing.rs core/model-api/src/drawing_annotations.rs tools/cli
# Dimension spacing (mitcad#1173) through the real UI, on a 40 x 30 x 20
# block mitcad-cli makes; its front view at 2:1 has its corners at
# (80, 100), (160, 100), (160, 140) and (80, 140) on the sheet, so a
# dimension between two corners of a side has its line_offset as far out
# as it lies beyond the view's box:
#   - the first dimension (the top corners) placed 12 mm above the view:
#     nothing to snap to, where it is clicked;
#   - the second (the bottom corners) clicked 13.5 mm below: its preview
#     snaps ("Drawing snap: ... x1") and it is placed at the first's
#     spacing, its line_offset as large as the first's;
#   - the third (the right side) clicked two spacings and 1 mm out: two
#     spacings out;
#   - the fourth (the left side) clicked with Alt 2.5 mm off the spacing:
#     where it is clicked ("free");
#   - the fourth dragged out near two spacings: it snaps to the third's
#     spacing (two of the first's).
# tools/ui-windows-drawing-spacing-test.ps1 runs the same on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-dimension-spacing-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-drawing-dimension-spacing.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/block.mitcad"

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# sheet_at x y: the screen position of a sheet point (mm), from the last
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100) in the window).
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
sheet_click() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2") click 1; ui_sync; }
sheet_hover() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2"); ui_sync; }
# sheet_alt_click x y: a click with Alt held.
sheet_alt_click() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2") keydown alt click 1 keyup alt; ui_sync; }

# sheet_drag_by "logged place" dx dy: a left drag from a logged place (an
# annotation's) by that many sheet millimetres.
sheet_drag_by() {
  local fx fy tx ty ox oy i
  read -r fx fy <<< "$(ui_logged_at "$1")"
  read -r ox oy <<< "$(sheet_at 0 0)"
  read -r tx ty <<< "$(sheet_at "$2" "$3")"
  tx=$((fx + tx - ox))
  ty=$((fy + ty - oy))
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4 5 6 7 8; do
    xdotool mousemove $((fx + (tx - fx) * i / 8)) $((fy + (ty - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
  ui_sync
}

# offset_of verb: the line_offset of the last "Drawing dimension <verb>:"
# line, without its sign.
offset_of() {
  grep -o "Drawing dimension $1: line_offset [-0-9.]*" "$UI_LOG" | tail -1 | awk '{ v = $NF; sub(/^-/, "", v); print v }'
}

# same a b what: two distances (mm) logged to 0.01 the same, or fail
# (their last digits may differ by the rounding of each).
same() {
  awk -v a="$1" -v b="$2" 'BEGIN { d = a - b; exit !(d <= 0.025 && d >= -0.025) }' ||
    ui_fail "$3: $1 is not $2"
  echo "ok   $3: $1"
}

no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

# dimension_between x1 y1 x2 y2: the two corners picked with Dimension.
dimension_between() {
  ui_step "a corner at $1, $2"              sheet_click "$1" "$2"
  ui_expect_new "Drawing picked point (1)" "the first corner picked"
  ui_step "a corner at $3, $4"              sheet_click "$3" "$4"
  ui_expect_new "Drawing picked point (2)" "the second corner picked"
  ui_expect_new "Drawing placing view" "its place is clicked next"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "block.mitcad" "the design opened"

echo "--- A drawing with the front view at 2:1"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog asks for the sheet"
ui_focus_dialog '^Create Drawing$'
ui_step "Template: ISO (Alt+T, Home)"     ui_key alt+t Home
ui_step "OK: A3, landscape, first angle"   ui_key Return
ui_focus_main
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing sheet view " "the sheet placed"
ui_expect_new "Drawing tool: Base View" "Base View is on"
ui_mark
ui_step "click the sheet at 120, 120"      sheet_click 120 120
ui_expect_new "Drawing view 1 at" "the front view drawn"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"

echo "--- The first dimension: where it is clicked"
ui_mark
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
dimension_between 80 140 160 140
ui_step "point 12 mm above the view"       sheet_hover 120 152
ui_expect_new "Drawing preview: dimension" "its lines follow the pointer"
ui_step "where it goes"                    sheet_click 120 152
ui_expect_new "Drawing dimension placed: line_offset" "its place logged"
ui_expect_new ", not snapped" "nothing to snap to: the first dimension"
ui_expect_new "Drawing annotations drawn: 1 = 40" "drawn: the block is 40 wide"
no_refusal
FIRST=$(offset_of placed)
awk -v v="$FIRST" 'BEGIN { exit !(v > 10.5 && v < 13.5) }' || ui_fail "the first dimension is not about 12 mm out: $FIRST"
echo "ok   the first dimension $FIRST mm out"

echo "--- The second: at the first's spacing"
ui_mark
dimension_between 80 100 160 100
ui_step "point 13.5 mm below the view"     sheet_hover 120 86.5
ui_expect_new "Drawing snap: " "the preview snapped"
ui_expect_new " mm x1" "one spacing out"
ui_step "where it goes"                    sheet_click 120 86.5
ui_expect_new "Drawing dimension placed: line_offset" "its place logged"
ui_expect_new "x1 of annotation 1" "snapped to the first's spacing"
ui_expect_new "Drawing annotations drawn: 1 = 40 | 2 = 40" "drawn"
no_refusal
same "$(offset_of placed)" "$FIRST" "the second's line_offset is the first's"

echo "--- The third: two spacings out"
ui_mark
dimension_between 160 140 160 100
X3=$(awk -v v="$FIRST" 'BEGIN { print 160 + 2 * v + 1 }')
ui_step "click two spacings and 1 mm out"  sheet_click "$X3" 120
ui_expect_new "x2 of annotation 2" "snapped to two spacings"
no_refusal
same "$(offset_of placed)" "$(awk -v v="$FIRST" 'BEGIN { printf "%.2f", 2 * v }')" "the third's line_offset is twice the first's"

echo "--- The fourth with Alt: where it is clicked"
ui_mark
dimension_between 80 140 80 100
X4=$(awk -v v="$FIRST" 'BEGIN { print 80 - v - 2.5 }')
ui_step "Alt+click 2.5 mm beyond a spacing" sheet_alt_click "$X4" 120
ui_expect_new "Drawing dimension placed: line_offset" "its place logged"
ui_expect_new ", free" "placed freely"
ui_expect_new "Drawing annotations drawn: 1 = 40 | 2 = 40 | 3 = 20 | 4 = 20" "drawn"
no_refusal
FOURTH=$(offset_of placed)
awk -v a="$FOURTH" -v b="$FIRST" 'BEGIN { d = a - b; exit !(d > 1 || d < -1) }' ||
  ui_fail "the fourth snapped with Alt: $FOURTH"
echo "ok   the fourth $FOURTH mm out, not at the spacing"

echo "--- The fourth dragged: it snaps"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
DX=$(awk -v v="$FIRST" 'BEGIN { print -(v - 2.5 + 0.5) }')
ui_mark
ui_step "drag it near two spacings out"    sheet_drag_by "Drawing annotation 4" "$DX" 0
ui_expect_new "Drawing annotation 4 dragged by" "the dimension dragged"
ui_expect_new "Drawing dimension moved: line_offset" "its new place logged"
ui_expect_new "x1 of annotation 3" "snapped to the third's spacing"
no_refusal
same "$(offset_of moved)" "$(awk -v v="$FIRST" 'BEGIN { printf "%.2f", 2 * v }')" "the dragged one's line_offset is twice the first's"

ui_finish 'UI drawing dimension spacing'
