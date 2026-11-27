#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# Sketch Dimension's angle between two lines measures the sector the value
# is placed in (mitcad#296), on a trapezoid mitcad-cli makes: Sketch1 (F1)
# on XY, its bottom c1 from (0, 0) to (40, 0) (fixed), c4 up to (30, 20), c6
# across to (10, 20) and the left side c8 from (10, 20) down to the corner
# (0, 0). The bottom's and the left side's directions make 116.57 degrees,
# the trapezoid's inner corner 63.43.
#   1. The bottom and the left side picked, the value placed inside the
#      corner: the value editor offers 63.43 (not 116.57); 60 entered, the
#      side turns to 60 degrees and the arc runs inside the corner, from the
#      bottom (0 degrees) to the side (60 degrees).
#   2. The same lines, the value placed in the neighbouring sector, between
#      the bottom's extension and the side: the supplement, 120 degrees
#      (driven: the corner is held), its arc from 180 to 60 degrees.
#   3. The saved design has both sides.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-angle-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-angle.XXXXXX)
FILE=$WORK/trapezoid.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [40, 0]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p3", "end": [30, 20]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p5", "end": [10, 20]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p7", "end": "p2"},
  {"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["c1"]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# double_click_at x y: a double click at a screen position.
double_click_at() { xdotool mousemove "$1" "$2" click --repeat 2 --delay 90 1; }

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
read -r tx ty <<< "$(ui_logged_at "Timeline Sketch1")"
ui_mark
ui_step "double-click Sketch1"           double_click_at "$tx" "$ty"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync
# Room left of the corner for the second value.
ui_step "show left of the corner"        zoom_to_show -15 -5 45 25

echo "--- The value inside the corner: the inner angle"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the bottom"                ui_sketch_click 20 0
ui_step "pick the left side"             ui_sketch_click 5 10
ui_step "place the value inside"         ui_sketch_click 8 4
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 63.43" "the inner angle offered, not the directions' 116.57"
ui_step "type 60"                        xdotool type --delay 40 "60"
ui_step "Enter"                          ui_key Return
ui_expect_new "Added dimension k" "a driving dimension"
ui_expect_new ": 60.00 deg, the arc from 0.0 to 60.0 deg about 0.00,0.00" \
  "the side turned to 60 degrees, the arc inside the corner"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- The value in the neighbouring sector: the supplement"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the bottom"                ui_sketch_click 20 0
ui_step "pick the left side"             ui_sketch_click 3 5.2
ui_step "place the value left of it"     ui_sketch_click -6 4
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 120.00" "the supplement offered"
ui_step "accept the value (Enter)"       ui_key Return
ui_expect_new "Added driven dimension k" "driven: the corner is held"
ui_expect_new ": 120.00 deg, the arc from 180.0 to 60.0 deg about 0.00,0.00" \
  "the arc from the bottom's extension to the side"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- The saved design"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
cat > "$WORK/check.json" << 'EOF2'
[
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "dimensions": [{"type": "angle", "a": "c1", "b": "c8", "reverse_a": false, "reverse_b": true},
                   {"type": "angle", "a": "c1", "b": "c8", "reverse_a": true, "reverse_b": true,
                    "driven": true}]}}}
]
EOF2
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved sides"; }
echo "ok   both sides saved"

ui_finish "UI sketch angle test"
