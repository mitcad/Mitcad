#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Sketch text, pattern edits and offsets (P3, P4) through the real UI, as a
# user works, with the volumes the model reports:
#   - A 60 x 40 plate with six holes patterned round its centre (made with
#     mitcad-cli), extruded 5. The sketch edited from the timeline: a
#     double-click on a hole the pattern made opens Edit Pattern; four
#     holes, and the body follows.
#   - Text: a drag with the Text tool draws its frame; a double-click on the
#     text opens Edit Text (height, alignment). The letters are profiles
#     outside the sketch: one extruded, the sketch (hidden since the plate
#     uses it) shown with its light bulb.
#   - Offset of an ellipse (a spline within a tolerance, kept at the
#     distance), then its distance changed in Edit Offset. The offset
#     curve dragged: it follows the cursor and the ellipse goes with it.
#
# Clicks are given in sketch millimetres (ui_sketch_click), from the
# placement the app logs.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-text-pattern-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-text.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Sketch1 (F1): the plate from (0, 0), a fixed point at its centre (p9), a
# hole of diameter 6 (c10) 15 left of it, patterned 6 times round the point
# (k8). Extrude1 (F2): the plate 5 thick, (2400 - 6 pi 9) 5 (as in
# tools/cli/tests/p34_text_patterns.json).
cat > "$WORK/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [30, 20], "fixed": true},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [15, 20], "diameter": 6},
  {"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": ["c10"], "center": "p9", "count": 6},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"expect": {"body": "F2.b0", "volume": 11151.769983530756}}
]
EOF
"$CLI" run "$WORK/plate.json" --save "$WORK/plate.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli plate"; }

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

# expect_near "log regex with two numbers" x y description: the last such
# line's numbers are within 1 of (x, y).
expect_near() {
  local at
  at=$(sed -n "s/.*$1.*/\\1 \\2/p" "$UI_LOG" | tail -1)
  [ -n "$at" ] || ui_fail "$4: not in the log"
  read -r ax ay <<< "$at"
  awk -v ax="$ax" -v ay="$ay" -v x="$2" -v y="$3" \
    'BEGIN { exit !((ax - x) ^ 2 + (ay - y) ^ 2 < 1.0) }' ||
    ui_fail "$4: at ($ax, $ay), expected near ($2, $3)"
  echo "ok   $4: ($ax, $ay)"
}

# double_click_sketch x y: a double click at a sketch point (mm).
double_click_sketch() { xdotool mousemove $(ui_sketch_at "$1" "$2") click --repeat 2 --delay 90 1; }

# newest_curve: the sketch curve with the highest number among the places
# logged since the mark ("Sketch entity F1/c17 at x,y").
newest_curve() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/.*Sketch entity F1\/c\([0-9]*\) at.*/\1/p' |
    sort -n | tail -1
}

ui_start_display
ui_start_app --open "$WORK/plate.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Edit Pattern: a double-click on a hole the pattern made"
ui_step "double-click Sketch1"             ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "sketch edited with the timeline rolled back"
ui_sync
ui_step "zoom out"                         zoom_to_show -10 -40 70 80
# The copy at 0 degrees: centre (45, 20), its right side at (48, 20).
ui_step "double-click the copy"            double_click_sketch 48 20
ui_expect_log ": sketch.edit_pattern" "the double-click opened Edit Pattern"
ui_expect_log "Panel Edit Pattern input quantity at" "the pattern's panel"
ui_step "quantity 4"                       ui_type_in "Panel Edit Pattern input quantity" "4"
ui_expect_log "Preview Edit Pattern: ok" "four holes previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited pattern k8: 4 in all" "the pattern has four holes"

echo "--- Text: a frame dragged with the Text tool"
ui_step "text (search)"                    ui_command "Text"
ui_step "drag the frame"                   ui_sketch_drag 0 50 60 70
ui_step "type the text"                    xdotool type --delay 40 "MIT"
ui_step "Enter"                            ui_key Return
ui_expect_log '"MIT" in a frame from (0, 50) to (60, 70)' "text in a frame"
# Texts are numbered with the sketch's other entities.
TEXT=$(sed -n 's/.*Added text \(t[0-9]*\) "MIT".*/\1/p' "$UI_LOG" | tail -1)
ui_step "end the tool (Esc)"               ui_key Escape

echo "--- Offset of an ellipse, then Edit Offset"
ui_step "ellipse (search)"                 ui_command "Ellipse"
ui_step "ellipse: centre"                  ui_sketch_click 30 -20
ui_step "ellipse: major axis"              ui_sketch_click 50 -20
ui_step "ellipse: minor radius"            ui_sketch_click 30 -10
ui_expect_log "radii 20 and 10 mm" "ellipse"
ui_step "end the tool (Esc)"               ui_key Escape
# A point of the ellipse off its axes: (30 + 20 cos 45, -20 + 10 sin 45).
ui_mark
ui_step "select the ellipse"               ui_sketch_click 44.142 -12.929
ui_expect_new "Selected: 1 sketch curve" "ellipse selected"
ELLIPSE=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  sed -n 's/.*Selected: 1 sketch curve \[sketch curve \(c[0-9]*\) of F1\].*/\1/p' | tail -1)
[ -n "$ELLIPSE" ] || ui_fail "the ellipse's id was not logged"
ui_step "offset (O)"                       ui_key o
ui_expect_log "Offset Curves: 1 sketch curve" "the ellipse went to the offset"
ui_expect_log "Preview Offset: ok" "offset previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Offset 1 curve(s) by 5 mm" "the ellipse offset"
ui_mark
ui_step "edit offset (search)"             ui_command "Edit Offset"
ui_expect_new "Sketch entity F1/$TEXT at" "the text and curves can be picked"
OFFSET_CURVE=$(newest_curve)
[ -n "$OFFSET_CURVE" ] || ui_fail "no sketch curves logged"
ui_step "pick the offset curve"            ui_click_logged "Sketch entity F1/c$OFFSET_CURVE"
ui_expect_log "Edit Offset Offset: 1 sketch curve" "the offset curve picked"
ui_step "distance 3"                       ui_type_in "Panel Edit Offset input distance" "3"
ui_expect_log "Preview Edit Offset: ok" "the new distance previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited offset k" "the offset's distance changed"

echo "--- The offset curve dragged: the ellipse goes with it"
# The offset's bottom, 3 below the ellipse's bottom (30, -30), dragged 15
# right (its top is near the plate's width dimension).
ui_mark
ui_step "drag the offset curve"            ui_sketch_drag 30 -33 45 -33
ui_expect_new "Dragged c$OFFSET_CURVE to" "the offset curve dragged"
expect_near "Dragged c$OFFSET_CURVE to (\([-0-9.]*\), \([-0-9.]*\))" 45 -33 \
  "the offset curve followed the cursor"
# A point of the moved ellipse off its axes: (45 - 20 cos 45, -20 - 10 sin 45).
ui_mark
ui_step "select the moved ellipse"         ui_sketch_click 30.858 -27.071
ui_expect_new "Selected: 1 sketch curve [sketch curve $ELLIPSE of F1]" "the ellipse moved with its offset"

echo "--- Edit Text: a double-click on the text"
ui_step "double-click the text"            ui_double_click_logged "Sketch entity F1/$TEXT"
ui_expect_log "Double-click on $TEXT: sketch.edit_text" "the double-click opened Edit Text"
ui_expect_log "Panel Edit Text input height at" "the text's panel"
ui_step "height 8"                         ui_type_in "Panel Edit Text input height" "8"
ui_step "centred"                          ui_choose "Panel Edit Text input align" 1
ui_expect_log "Edit Text: Horizontal Alignment = Center" "centre chosen"
ui_expect_log "Preview Edit Text: ok" "the text previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited text $TEXT: \"MIT\", Droid Sans, center" "the text edited"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- The body follows the pattern"
ui_step "finish sketch (Ctrl+Enter)"       ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
# Four holes: (2400 - 4 pi 9) 5.
ui_expect_volume "Body Body1 (F2.b0): volume \([0-9.]*\) mm3" 11434.513322353838 \
  "the plate with four holes"

echo "--- The letters are profiles outside the sketch"
# The plate uses the sketch, so it is hidden (mitcad#7) and the view
# leaves the text under the ViewCube: its light bulb shows it, with all
# its profiles, and a fit takes in the text.
ui_mark
ui_step "show Sketch1 (Browser)"           ui_click_logged "Browser eye Root/Sketches/Sketch1"
ui_expect_new "Visibility Root/Sketches/Sketch1: shown" "the sketch shown"
ui_step "fit (F6)"                         ui_key F6
ui_step "extrude (E)"                      ui_key e
ui_expect_log "Pick profile F1/r{$TEXT.g0.c0}" "the M can be picked"
ui_expect_log "Extrude Profiles: 1 profile" "the newest profile taken"
PRESET=$(grep "Extrude Profiles: 1 profile" "$UI_LOG" | tail -1 | sed -n 's/.*\[profile \(.*\) of F1\].*/\1/p')
if [ "$PRESET" != "r{$TEXT.g1.c0}" ]; then
  ui_step "take $PRESET away"              ui_click_pick "profile F1/$PRESET"
  ui_step "pick the I"                     ui_click_pick "profile F1/r{$TEXT.g1.c0}"
fi
ui_expect_log "Extrude Profiles: 1 profile [profile r{$TEXT.g1.c0} of F1]" "the I picked"
ui_step "distance 2"                       ui_type_in "Panel Extrude input distance" "2"
ui_expect_log "Preview Extrude: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
# The bundled font's I is a rectangle: 2 x its area at height 8 (mitcad-cli,
# the same text extruded).
ui_expect_volume "New body Body2 (F3.b0): volume \([0-9.]*\) mm3" 10.870727539062502 "the I extruded"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI text and pattern test"
