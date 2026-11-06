#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs tools/cli
# A section's line and a break's sides snap (mitcad#1362) through the real
# UI, on a plate that mitcad-cli makes: 90 x 50 x 5 with a hole of 16
# through it at (30, 25), a drawing of it with its top view at 2:1 (its
# automatic centre marks on), so that the plate's point (x, y) lies at
# (120 + 2x, 100 + 2y) on the sheet and the hole's centre at (180, 150):
#   1. Section View: a click near the hole's centre snaps onto it, a
#      second on the hole's vertical centre line onto that line: a
#      vertical section line through the centre; placed beside the view,
#      a second view.
#   2. Section View: the centre, then a click about 46 degrees from it
#      near nothing: turned to 45 degrees ("Drawing snap: section line at
#      45 degrees"); Esc.
#   3. Section View with Shift held for both clicks: both where clicked
#      (no snap, no angle); Esc.
#   4. Break: its first side near the hole's centre, its second near the
#      plate's top edge's middle (45, 50): both snapped, the break made.
# tools/ui-windows-drawing-section-snap-test.ps1 is the Windows
# counterpart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-section-snap-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-section-snap.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE=$WORK/plate.mitcad

cat > "$WORK/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 90, "height": 50},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 25], "diameter": 16},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [210, 150], "scale": "2:1"}
]
EOF
"$CLI" run "$WORK/plate.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
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
# plate_at x y: the sheet point of the plate's point (x, y) in its top view.
plate_at() { awk -v x="$1" -v y="$2" 'BEGIN { printf "%.2f %.2f\n", 120 + 2 * x, 100 + 2 * y }'; }
plate_click() { ui_sync; xdotool mousemove $(sheet_at $(plate_at "$1" "$2")) click 1; ui_sync; }
# plate_shift_click x y: a click with Shift held.
plate_shift_click() {
  ui_sync
  xdotool mousemove $(sheet_at $(plate_at "$1" "$2")) keydown shift click 1 keyup shift
  ui_sync
}
sheet_click() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2") click 1; ui_sync; }

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

# point_line n: the newest "Drawing point <n> at …" line since the mark.
point_line() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Drawing point $1 at .*" | tail -1; }

# tool "name": the drawing tool from the bar.
tool() {
  ui_mark
  settled "Drawing tool $1"
  ui_step "$1"                             ui_click_logged "Drawing tool $1"
  ui_expect_new "Drawing tool: $1" "the $1 tool is on"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "plate.mitcad" "the design opened"

ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
ui_expect_new "Drawing sheet drawn: 1 views" "its top view"
ui_expect_new "Drawing sheet view " "the sheet placed"

echo "--- 1. A vertical section line through the hole's centre"
tool "Section View"
ui_mark
ui_step "near the hole's centre"           plate_click 30.3 25.2
ui_expect_new "Drawing point snapped to " "the click snapped"
ui_expect_new "Drawing point 1 at 180.0,150.0 (snapped)" "onto the hole's centre"
ui_step "on its vertical centre line"      plate_click 30.3 33
ui_expect_new "Drawing point 2 at 180.0," "onto the centre line: a vertical line"
line=$(point_line 2)
case "$line" in
  *"(snapped)") echo "ok   $line" ;;
  *) ui_fail "the second point did not snap: $line" ;;
esac
ui_expect_new "Drawing placing view" "the section placed next"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "where it goes, beside the view"   sheet_click 360 150
ui_expect_new "Drawing sheet drawn: 2 views" "the section view made"
no_refusal

echo "--- 2. Near 45 degrees: 45 degrees"
tool "Section View"
ui_mark
ui_step "near the hole's centre"           plate_click 30.3 25.2
ui_expect_new "Drawing point 1 at 180.0,150.0 (snapped)" "onto the hole's centre"
ui_step "about 46 degrees from it"         plate_click 50 46
ui_expect_new "Drawing snap: section line at 45 degrees" "turned to 45 degrees"
line=$(point_line 2)
case "$line" in
  *"(at 45 degrees)") ;;
  *) ui_fail "the second point not at 45 degrees: $line" ;;
esac
# 45 degrees from (180, 150), as long as the line clicked.
awk -v l="$line" 'BEGIN {
  split(l, w, " "); split(w[5], p, ",")
  exit !((p[1] - 180 > 0) && ((p[1] - 180) - (p[2] - 150)) ^ 2 < 0.04) }' ||
  ui_fail "the second point is not on the 45 degree line: $line"
echo "ok   $line"
ui_step "drop it (Esc)"                    ui_key Escape

echo "--- 3. Shift held: where it is clicked"
ui_mark
ui_step "near the centre, Shift held"      plate_shift_click 30.75 25.5
ui_step "near upright, Shift held"         plate_shift_click 31 60
ui_expect_new "Drawing point 2 at " "both clicks taken"
grep -q "Drawing point snapped to \|Drawing snap: section line" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "a click with Shift snapped"
for n in 1 2; do
  line=$(point_line "$n")
  case "$line" in
    *"(snapped)"* | *"degrees)"* | *" at 180.0,"*) ui_fail "click $n with Shift moved: $line" ;;
  esac
  echo "ok   $line"
done
ui_step "drop it (Esc)"                    ui_key Escape
ui_step "end the tool (Esc)"               ui_key Escape

echo "--- 4. A break's sides snap the same way"
tool "Break"
ui_mark
ui_step "near the hole's centre"           plate_click 30.3 25.2
ui_expect_new "Drawing point 1 at 180.0,150.0 (snapped)" "onto the hole's centre"
ui_step "near the top edge's middle"       plate_click 45.3 49.7
ui_expect_new "Drawing point 2 at 210.0,200.0 (snapped)" "onto the edge's middle"
ui_expect_new '"add_break"' "the break asked for"
no_refusal

ui_finish "UI drawing section snap test"
