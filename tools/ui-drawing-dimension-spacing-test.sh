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
#     spacing (two of the first's);
#   - a datum label on the top edge, its square dragged so that its
#     leader runs at about 47 degrees: the leader snaps to 45 (mitcad#1294);
#     dragged again with Shift held, near enough to snap: where it is
#     dropped;
#   - its triangle dragged along the edge (mitcad#1295): the tip slides
#     along it, the square follows, undo puts both back; its square
#     dragged: the square alone moves;
#   - its triangle dragged past the edge's end (mitcad#1296): on the
#     edge's line, beyond the end; a datum clicked inside the front face:
#     the face picked, a dot; its Terminator switched to Triangle in its
#     panel.
# tools/ui-windows-drawing-spacing-test.ps1 runs the same on Windows (the
# dimensions).
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

# A datum label's leader (mitcad#1294) runs from its triangle's apex,
# 2.8 mm square off the edge, to its square's lower middle (text_offset
# from the tip) while the square lies above: its run is (u, v - 2.8).
# leader_skew: how far the last moved one's run is from 45 degrees, mm.
leader_skew() {
  grep -o "Drawing leader moved: datum text_offset [-0-9.]*,[-0-9.]*" "$UI_LOG" | tail -1 |
    awk '{ split($NF, t, ","); d = t[1] - (t[2] - 2.8); if (d < 0) d = -d; print d }'
}

echo "--- A datum's square dragged near 45 degrees: its leader snaps"
ui_mark
ui_step "Datum Label"                      ui_click_logged "Drawing tool Datum Label"
ui_expect_new "Drawing tool: Datum Label" "the datum tool is on"
ui_step "the front view's top edge"        sheet_click 150 139.5
ui_expect_new "Drawing picked line (1)" "the edge picked"
ui_step "where its label goes"             sheet_click 135 158
ui_expect_new '"type":"datum"' "a datum asked for"
ui_expect_new "5 = A" "the datum drawn"
ui_expect_new "Drawing annotation 5 tip at" "its triangle logged"
no_refusal
# The pick names the edge: its triangle at the edge's middle (120, 140),
# the square's lower middle where clicked, 15 right and 18 up of it.
read -r TIPX TIPY <<< "$(grep -F "Drawing annotation 5 tip at " "$UI_LOG" | tail -1 |
  sed -n 's/.* at \([0-9]*\),\([0-9]*\).*/\1 \2/p')"
read -r MIDX MIDY <<< "$(sheet_at 120 140)"
awk -v a="$((X + TIPX * UI_SCALE))" -v b="$MIDX" -v c="$((Y + TIPY * UI_SCALE))" -v d="$MIDY" \
  'BEGIN { exit !((a - b) ^ 2 <= 9 && (c - d) ^ 2 <= 9) }' ||
  ui_fail "the datum's triangle is not at the edge's middle: $TIPX,$TIPY"
echo "ok   its triangle at the edge's middle"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
# Its square 5 right and 6.25 up: its lower middle 20 right and 24.25 up
# of the tip, the leader from the apex (2.8 up) 47 degrees up and right.
ui_mark
ui_step "drag its square near 45 degrees"  sheet_drag_by "Drawing annotation 5" 5 6.25
ui_expect_new "Drawing annotation 5 dragged by" "the square dragged"
ui_expect_new "Drawing snap: leader at 45 degrees" "the drag snapped"
ui_expect_new "Drawing leader moved: datum text_offset" "its new place logged"
ui_expect_new "snapped to 45 degrees" "its leader at 45 degrees"
no_refusal
SKEW=$(leader_skew)
awk -v d="$SKEW" 'BEGIN { exit !(d <= 0.02) }' || ui_fail "the leader is $SKEW mm off 45 degrees"
echo "ok   the leader runs at 45 degrees"

# With Shift held, 1.5 mm further right (about 42 degrees, near enough
# to snap): where it is dropped. At this zoom 1.5 mm is under the sheet's
# 3 pixels that make a press a drag, so the pointer goes 10 mm out first
# and then back to 1.5 mm.
sheet_shift_drag_by() {
  local fx fy ox oy wx wy tx ty i
  read -r fx fy <<< "$(ui_logged_at "$1")"
  read -r ox oy <<< "$(sheet_at 0 0)"
  read -r wx wy <<< "$(sheet_at 10 0)"
  read -r tx ty <<< "$(sheet_at "$2" "$3")"
  wx=$((fx + wx - ox))
  wy=$((fy + wy - oy))
  tx=$((fx + tx - ox))
  ty=$((fy + ty - oy))
  ui_apart "$fx" "$fy"
  xdotool keydown shift
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4; do
    xdotool mousemove $((fx + (wx - fx) * i / 4)) $((fy + (wy - fy) * i / 4))
    sleep 0.08
  done
  for i in 1 2 3 4; do
    xdotool mousemove $((wx + (tx - wx) * i / 4)) $((wy + (ty - wy) * i / 4))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
  ui_sync
  xdotool keyup shift
}
ui_mark
ui_step "Shift+drag it 1.5 mm right"       sheet_shift_drag_by "Drawing annotation 5" 1.5 0
ui_expect_new "Drawing annotation 5 dragged by" "the square dragged"
ui_expect_new "Drawing leader moved: datum text_offset" "its new place logged"
ui_expect_new ", free" "placed freely"
no_refusal
SKEW=$(leader_skew)
awk -v d="$SKEW" 'BEGIN { exit !(d > 0.5) }' || ui_fail "the leader snapped with Shift: $SKEW mm off 45 degrees"
echo "ok   with Shift the leader is $SKEW mm off 45 degrees, not snapped"

# logged_xy "logged place": its last logged window position, px.
logged_xy() {
  grep -F "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9]*\),\([0-9]*\).*/\1 \2/p'
}
# settled_place "logged place": waits until its logged position stops
# changing (a panel laid out again).
settled_place() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "'$1' did not settle"
}
# px_per_mm: window px per sheet mm, from the last logged sheet.
px_per_mm() {
  grep -o "Drawing sheet view [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1 |
    awk '{ split($4, o, ","); split($5, a, ","); print (a[1] - o[1]) / 100 }'
}
# within a b px what: two window positions this near, or fail.
within() {
  awk -v a="$1" -v b="$2" -v px="$3" 'BEGIN { d = a - b; exit !(d <= px && d >= -px) }' ||
    ui_fail "$4: $1 is not $2"
  echo "ok   $4"
}

echo "--- The datum's triangle dragged along its edge (mitcad#1295)"
PPM=$(px_per_mm)
read -r TX0 TY0 <<< "$(logged_xy "Drawing annotation 5 tip")"
read -r SX0 SY0 <<< "$(logged_xy "Drawing annotation 5")"
[ -n "$TY0" ] && [ -n "$SY0" ] || ui_fail "the datum's triangle or square was not logged"
ui_mark
ui_step "drag its triangle 20 left, 3 up"  sheet_drag_by "Drawing annotation 5 tip" -20 3
ui_expect_new "Drawing annotation 5 tip dragged by" "the triangle dragged"
ui_expect_new '"slide":' "its slide along the edge asked for"
ui_expect_new "Drawing annotation 5 tip at" "drawn again"
no_refusal
read -r TX1 TY1 <<< "$(logged_xy "Drawing annotation 5 tip")"
read -r SX1 SY1 <<< "$(logged_xy "Drawing annotation 5")"
within "$TY1" "$TY0" 1 "the tip stays on the edge"
within "$((TX1 - TX0))" "$(awk -v p="$PPM" 'BEGIN { printf "%d", -20 * p }')" 3 "the tip slid 20 mm along it"
within "$((SX1 - SX0))" "$((TX1 - TX0))" 2 "the square followed it"
within "$SY1" "$SY0" 1 "the square kept its height"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing annotation 5 tip at" "drawn again"
read -r TX2 TY2 <<< "$(logged_xy "Drawing annotation 5 tip")"
read -r SX2 SY2 <<< "$(logged_xy "Drawing annotation 5")"
within "$TX2" "$TX0" 1 "undo put the tip back"
within "$SX2" "$SX0" 1 "and the square"

echo "--- The datum's square dragged: the square alone moves"
ui_mark
ui_step "drag its square 5 right"          sheet_drag_by "Drawing annotation 5" 5 0
ui_expect_new "Drawing annotation 5 dragged by" "the square dragged"
ui_expect_new "Drawing leader moved: datum text_offset" "its new place logged"
ui_expect_new "Drawing annotation 5 tip at" "drawn again"
no_refusal
read -r TX3 TY3 <<< "$(logged_xy "Drawing annotation 5 tip")"
read -r SX3 SY3 <<< "$(logged_xy "Drawing annotation 5")"
within "$TX3" "$TX0" 1 "the tip stayed"
within "$TY3" "$TY0" 1 "on its place"
within "$((SX3 - SX0))" "$(awk -v p="$PPM" 'BEGIN { printf "%d", 5 * p }')" 3 "the square moved 5 mm"

echo "--- The datum's triangle past the edge's end: on its extension (mitcad#1296)"
ui_mark
# From x 120 on the top edge (80 to 160) 50 mm right: 10 past its end.
ui_step "drag its triangle 50 right"       sheet_drag_by "Drawing annotation 5 tip" 50 0
ui_expect_new "Drawing annotation 5 tip dragged by" "the triangle dragged"
ui_expect_new '"slide":' "its slide asked for"
ui_expect_new "Drawing annotation 5 tip at" "drawn again"
no_refusal
read -r TX4 TY4 <<< "$(logged_xy "Drawing annotation 5 tip")"
read -r EX EY <<< "$(sheet_at 160 140)"
within "$TY4" "$TY0" 1 "the tip on the edge's line"
awk -v t="$TX4" -v e="$(( (EX - X) / UI_SCALE ))" -v p="$PPM" 'BEGIN { exit !(t > e + 5 * p) }' ||
  ui_fail "the tip is not past the edge's end: $TX4"
echo "ok   the tip past the edge's end, on its extension line"

echo "--- A datum on the front face: a dot, its Terminator switched (mitcad#1296)"
ui_mark
ui_step "Datum Label"                      ui_click_logged "Drawing tool Datum Label"
ui_expect_new "Drawing tool: Datum Label" "the datum tool is on"
ui_step "inside the front face"            sheet_click 100 120
ui_expect_new "Drawing picked face (1)" "the face picked, face on"
ui_step "where its label goes"             sheet_click 112 130
ui_expect_new '"on_face":' "a datum on the face asked for"
ui_expect_new "6 = B" "the datum drawn"
ui_expect_new "Drawing annotation 6 tip at" "its terminator logged"
no_refusal
grep "Drawing annotation 6 tip at" "$UI_LOG" | tail -1 | grep -q "(dot)" || ui_fail "the face's datum has no dot"
echo "ok   a dot on the face"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
ui_mark
# Apart from the placing click just before (its letter is near it): no
# double-click.
apart_click() {
  local x y
  read -r x y <<< "$(ui_logged_at "$1")"
  ui_apart "$x" "$y"
  xdotool mousemove "$x" "$y" click 1
}
ui_step "select it"                        apart_click "Drawing annotation 6"
ui_expect_new "Drawing annotation 6 selected" "the datum selected"
# Its panel as datum A's was (its places logged only when they change).
settled_place "Drawing panel drawingAnnotationTerminator"
ui_mark
ui_step "Terminator: Triangle (Up)"        ui_click_logged "Drawing panel drawingAnnotationTerminator"
ui_key Up Return
ui_expect_new '"terminator":"triangle"' "a triangle asked for"
ui_expect_new "Drawing annotation 6 tip at" "drawn again"
no_refusal
grep "Drawing annotation 6 tip at" "$UI_LOG" | tail -1 | grep -q "(triangle)" || ui_fail "the datum kept its dot"
echo "ok   switched to a triangle"

ui_finish 'UI drawing dimension spacing'
