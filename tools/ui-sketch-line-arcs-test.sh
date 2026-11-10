#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# The Line tool's arcs and the Midpoint Line (mitcad#1393), through the real
# UI, on a design mitcad-cli makes: Sketch1 (F1) on XY with a line c1 from
# p2 (-40, -20) to p3 (-10, -20), points p4 (-60, 40) and p5 (60, 40) that
# widen the fitted view, p6 (20, 20) for a midpoint, p7 (30, -40) and
# p8 (50, -40) for a chain, p9 (-40, 10) and p10 (-20, 10) for another.
#   1. The Line tool pressed on c1's end p3 and dragged along it draws a
#      tangent arc from c1 (no chain before it).
#   2. A chain's line from p7 to p8, then a drag from its end across it,
#      with 15 typed while the button is down: a normal arc with a radius
#      dimension of 15 mm.
#   2b. A chain's line from (-40, 10) to (-20, 10), then a drag from its
#      end up across it with 10, Tab and 270 typed: the normal arc leaves
#      upward, toward the pointer, and sweeps 270 degrees round (-10, 10)
#      to (-10, 0), below the line (mitcad#1608).
#   3. Midpoint Line: a click on p6 and one at (5, 10): the line's middle
#      is p6, its ends as far on either side.
#   4. Saved, the file has the normal arc's radius dimension and the
#      midpoint constraint.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-line-arcs-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-line-arcs.XXXXXX)
FILE=$WORK/line-arcs.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [-40, -20], "end": [-10, -20]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-60, 40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [20, 20]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [30, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [50, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-40, 10]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-20, 10]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# arc_drag x1 y1 x2 y2 text [sweep]: a left drag between sketch points (mm)
# that types the text halfway, with the button still down, and then Tab
# and the sweep.
arc_drag() {
  local i fx fy tx ty
  read -r fx fy <<< "$(ui_sketch_at "$1" "$2")"
  read -r tx ty <<< "$(ui_sketch_at "$3" "$4")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4; do
    xdotool mousemove $((fx + (tx - fx) * i / 8)) $((fy + (ty - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool type --delay 40 "$5"
  ui_sync
  if [ -n "${6:-}" ]; then
    xdotool key Tab
    ui_sync
    xdotool type --delay 40 "$6"
    ui_sync
  fi
  for i in 5 6 7 8; do
    xdotool mousemove $((fx + (tx - fx) * i / 8)) $((fy + (ty - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync

echo "--- A tangent arc from the end of a line"
ui_mark
ui_step "line tool (L)"                  ui_key l
ui_step "drag from c1's end along it"    ui_sketch_drag -10 -20 10 -10
ui_expect_new "Line: an arc from the end p3 of c1" "the press on c1's end starts an arc from c1"
ui_expect_new "Line arc: tangent" "a drag along the line chooses a tangent arc"
ui_expect_new "Added tangent arc c" "the tangent arc is added"
ui_step "end the chain (Esc)"            ui_key Escape
ui_expect_new "Line chain ended" "the chain ended"

echo "--- A normal arc with a typed radius from a chain's line"
ui_mark
ui_step "chain: the start on p7"         ui_sketch_click 30 -40
ui_step "chain: the end on p8"           ui_sketch_click 50 -40
ui_expect_new "Added line c" "the chain's line"
ui_expect_new " from (30, -40) to (50, -40)" "from p7 to p8"
ui_step "drag across it, typing 15"      arc_drag 50 -40 60 -25 15
ui_expect_new "Line arc: normal" "a drag across the line chooses a normal arc"
ui_expect_new "Added normal arc c" "the normal arc is added"
ui_expect_new ", radius 15.000 mm, dimensioned" "the typed radius dimensions it"
ui_step "end the chain (Esc)"            ui_key Escape
ui_step "end the tool (Esc)"             ui_key Escape
grep -q "Sketch command sketch.arc refused" "$UI_LOG" && ui_fail "an arc was refused"

echo "--- A normal arc of a typed radius and a sweep of 270 degrees"
ui_mark
ui_step "line tool (L)"                  ui_key l
ui_step "chain: the start on p9"         ui_sketch_click -40 10
ui_step "chain: the end on p10"          ui_sketch_click -20 10
ui_expect_new " from (-40, 10) to (-20, 10)" "the chain's line"
ui_step "drag up across it, typing 10 and 270" arc_drag -20 10 -15 25 10 270
ui_expect_new "Line arc: normal" "a drag across the line chooses a normal arc"
ui_expect_new ", radius 10.000 mm, dimensioned" "the typed radius dimensions it"
ui_expect_new "sweeps 270.0 degrees" "it leaves upward, toward the pointer, and sweeps 270 degrees (mitcad#1608)"
ui_step "end the chain (Esc)"            ui_key Escape
ui_step "end the tool (Esc)"             ui_key Escape
grep -q "Sketch command sketch.arc refused" "$UI_LOG" && ui_fail "an arc was refused"

echo "--- Midpoint Line"
ui_mark
ui_step "midpoint line (search)"         ui_command "Midpoint Line"
ui_expect_new "Sketch tool Midpoint Line" "Midpoint Line started"
ui_step "the midpoint on p6"             ui_sketch_click 20 20
# Low in the view: a click at (40, 30) hit the orientation cube in a small window.
ui_step "an end"                         ui_sketch_click 5 10
ui_expect_new "Added midpoint line c" "the midpoint line is added"
read -r ax ay bx by mx my <<< "$(grep -o "Added midpoint line c[0-9]* from ([^)]*) to ([^)]*) about ([^)]*)" "$UI_LOG" |
  tail -1 | sed 's/.* from (\([^,]*\), \([^)]*\)) to (\([^,]*\), \([^)]*\)) about (\([^,]*\), \([^)]*\))/\1 \2 \3 \4 \5 \6/')"
awk -v ax="$ax" -v ay="$ay" -v bx="$bx" -v by="$by" -v mx="$mx" -v my="$my" 'BEGIN {
  dx = (ax + bx) / 2 - mx; dy = (ay + by) / 2 - my; if (dx < 0) dx = -dx; if (dy < 0) dy = -dy
  exit !(mx == 20 && my == 20 && dx <= 1e-5 && dy <= 1e-5) }' ||
  ui_fail "the midpoint line runs from ($ax, $ay) to ($bx, $by) about ($mx, $my), not about (20, 20)"
echo "ok   its middle is the first click, p6 at (20, 20)"
ui_step "end the tool (Esc)"             ui_key Escape
grep -q "Midpoint Line not added" "$UI_LOG" && ui_fail "the midpoint line was refused"

ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch finished"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
grep -qE '"type": ?"radius"' "$FILE" || ui_fail "the saved sketch has no radius dimension"
echo "ok   the saved sketch has the arc's radius dimension"
grep -qE '"type": ?"midpoint"' "$FILE" || ui_fail "the saved sketch has no midpoint constraint"
echo "ok   the saved sketch has the midpoint constraint"

ui_finish "UI sketch line arcs test"
