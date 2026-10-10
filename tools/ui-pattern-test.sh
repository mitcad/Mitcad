#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands
# Rectangular, Circular and Path Pattern and Mirror (U4) through the real
# UI, on a 20 x 20 x 10 block with a 5 mm hole cut through it at (5, 5):
#   - a rectangular pattern of the hole's feature, picked in the timeline;
#   - patterns of the body along X, about Z (an instance suppressed by its
#     dot in the view, P9, and back again) and along a sketch line;
#   - the body mirrored about YZ and joined; the pattern edited.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pattern-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-pattern.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
PI=3.14159265358979
HOLE=$(awk -v pi=$PI 'BEGIN { printf "%.6f", pi * 2.5^2 * 10 }')

# Sketch1 (F1), Extrude1 (F2): the block, Body1; Sketch2 (F3), Extrude2
# (F4): the hole, cut through all; Sketch3 (F5): the path (0, -40)-(100, -40).
cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [5, 5], "diameter": 5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F5", "start": [0, -40], "end": [100, -40]}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_step "fit (F6)"                         ui_key F6
BLOCK=$(awk -v h="$HOLE" 'BEGIN { printf "%.3f", 4000 - h }')

echo "--- A second hole: a pattern of the cut, picked in the timeline"
ui_step "rectangular pattern (search)"     ui_command "Rectangular Pattern"
ui_step "Features"                         ui_choose "Panel Rectangular Pattern input object_type" 1
ui_expect_log "Rectangular Pattern: Pattern Type = Features" "pattern of features"
ui_step "pick Extrude2 in the timeline"    ui_click_logged "Timeline Extrude2"
ui_expect_log "Rectangular Pattern Objects: 1 feature [feature F4]" "the cut picked"
ui_step "the direction input"              ui_click_logged "Panel Rectangular Pattern input direction1"
ui_step "pick the X axis"                  ui_click_logged "Datum x"
ui_step "quantity 2"                       ui_type_in "Panel Rectangular Pattern input quantity1" "2"
ui_step "distance 10"                      ui_type_in "Panel Rectangular Pattern input distance1" "10"
ui_expect_log "Preview Rectangular Pattern: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added rectangular pattern of 1 features" "pattern added"
ui_expect_volume "Body Body1 (F2.b0): volume $BLOCK -> \([0-9.]*\) mm3" \
  "$(awk -v h="$HOLE" 'BEGIN { printf "%.6f", 4000 - 2 * h }')" "two holes"
BLOCK=$(awk -v h="$HOLE" 'BEGIN { printf "%.3f", 4000 - 2 * h }')

echo "--- The body along X: 3 at 30 mm"
ui_step "rectangular pattern (search)"     ui_command "Rectangular Pattern"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_expect_log "Rectangular Pattern Objects: 1 body [body F2.b0]" "body picked"
ui_step "the direction input"              ui_click_logged "Panel Rectangular Pattern input direction1"
ui_step "pick the X axis"                  ui_click_logged "Datum x"
ui_expect_log "Manipulator Rectangular Pattern distance1 at" "the distance has an arrow"
ui_step "distance 30"                      ui_type_in "Panel Rectangular Pattern input distance1" "30"
ui_expect_log "Preview Rectangular Pattern: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body Body3 (F7.b1): volume \([0-9.]*\) mm3" "$BLOCK" "the third copy"

echo "--- Edited to 2 copies"
ui_step "double-click the pattern"         ui_double_click_logged "Timeline RectangularPattern2"
ui_expect_log "Editing F7 with Rectangular Pattern" "editing the pattern"
ui_expect_log "Rectangular Pattern Quantity: 3" "the quantity came from the feature"
ui_step "quantity 2"                       ui_type_in "Panel Rectangular Pattern input quantity1" "2"
ui_expect_log "Preview Rectangular Pattern: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F7" "pattern edited"
ui_expect_log "Removed body Body3 (F7.b1)" "one copy fewer"

echo "--- The body about Z: 4 in a full turn"
ui_step "circular pattern (search)"        ui_command "Circular Pattern"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_step "the axis input"                   ui_click_logged "Panel Circular Pattern input axis"
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_log "Circular Pattern Axis: 1 axis [axis z]" "Z picked"
ui_expect_log "Preview Circular Pattern: ok" "previewed"
# A dot at each instance (P9): a click suppresses the second.
ui_expect_log "Manipulator Circular Pattern suppressed.2 at" "the instances have dots"
ui_mark
ui_step "click the second instance's dot"  ui_click_logged "Manipulator Circular Pattern suppressed.2"
ui_expect_new "Manipulator Circular Pattern suppressed.2 toggled" "toggled"
ui_expect_new "Circular Pattern Suppressed: 2" "the second instance suppressed"
ui_expect_new "Manipulator Circular Pattern suppressed.2 (off) at" "its dot is hollow"
ui_expect_new "Preview Circular Pattern: ok" "previewed without it"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body [A-Za-z0-9]* (F8.b2): volume \([0-9.]*\) mm3" "$BLOCK" "the fourth copy"
grep -q "New body [A-Za-z0-9]* (F8.b1)" "$UI_LOG" && ui_fail "the suppressed instance was made"
echo "ok   the second instance is not made"
ui_mark
ui_step "double-click the pattern"         ui_double_click_logged "Timeline CircularPattern1"
ui_expect_new "Editing F8 with Circular Pattern" "a pattern with a suppressed instance can be edited"
ui_expect_new "Manipulator Circular Pattern suppressed.2 (off) at" "the suppressed instance's dot"
ui_step "click it again"                   ui_click_logged "Manipulator Circular Pattern suppressed.2 (off)"
ui_expect_new "Circular Pattern Suppressed: " "nothing suppressed"
ui_expect_new "Preview Circular Pattern: ok" "previewed with it"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F8" "edited"
ui_expect_volume "New body [A-Za-z0-9]* (F8.b1): volume \([0-9.]*\) mm3" "$BLOCK" "the second instance is back"

echo "--- The body along the sketch line"
ui_step "pattern on path (search)"         ui_command "Pattern on Path"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_step "the path input"                   ui_click_logged "Panel Pattern on Path input path"
ui_step "pick the line"                    ui_click_logged "Sketch entity F5/c1"
ui_step "distance 25"                      ui_type_in "Panel Pattern on Path input distance" "25"
ui_expect_log "Preview Pattern on Path: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body [A-Za-z0-9]* (F9.b1): volume \([0-9.]*\) mm3" "$BLOCK" "the third copy along the path"

echo "--- Mirror about YZ, joined"
ui_step "mirror (search)"                  ui_command "Mirror"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_step "the plane input"                  ui_click_logged "Panel Mirror input plane"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_step "Join"                             ui_choose "Panel Mirror input operation" 1
ui_expect_log "Mirror: Operation = Join" "join chosen"
ui_expect_log "Preview Mirror: ok" "previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume $BLOCK -> \([0-9.]*\) mm3" \
  "$(awk -v b="$BLOCK" 'BEGIN { printf "%.6f", 2 * b }')" "the body and its mirror image"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI pattern test"
