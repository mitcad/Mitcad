#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/pattern.rs tools/cli
# Rectangular Pattern's Direction 3 (mitcad#1446) through the real UI, on
# a 20 x 20 x 10 block at the origin (Body1, F2.b0; its centre (10, 10, 5)):
#   - Rectangular Pattern of Body1: Direction 1 along X, 2 at 30 mm;
#     Direction 2 along Y, 3 at 30 mm (the preview's 6 elements); Direction
#     3's rows hidden until its axis is picked, then along Z, 4 at 30 mm,
#     with its arrow along Z; the preview's 24 elements;
#   - OK: the added line names the three directions;
#   - saved; mitcad-cli reads 24 bodies and the copies' centres, element
#     e = i + 2 j + 6 k at (30 i, 30 j, 30 k) from the block (body
#     F3.b(e-1)).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pattern-grid-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-pattern-grid.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# Sketch1 (F1), Extrude1 (F2): the block, Body1.
cat > "$WORK/block.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_hidden "Panel <command> input <id>" description: the input's last
# logged place says it is hidden (as tools/ui-pattern-options-test.sh).
expect_hidden() {
  for _ in $(seq 1 30); do
    grep -F "$1 at " "$UI_LOG" | tail -1 | grep -q '(hidden)$' && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: the last place of '$1' is not hidden"
}

# expect_shown "Panel <command> input <id>" description: the input's last
# logged place says it is shown.
expect_shown() {
  for _ in $(seq 1 30); do
    grep -F "$1 at " "$UI_LOG" | tail -1 | grep -q '[0-9]$' && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: the last place of '$1' is not shown"
}

ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Direction 1 along X and Direction 2 along Y"
ui_mark
ui_step "rectangular pattern (search)"     ui_command "Rectangular Pattern"
ui_expect_new "Command Rectangular Pattern started" "Rectangular Pattern started"
ui_expect_new "direction_type2=one, direction_type3=one" "Direction 3 has its Direction Type"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_expect_new "Rectangular Pattern Objects: 1 body [body F2.b0]" "the block picked"
expect_hidden "Panel Rectangular Pattern input quantity3" "Quantity 3 hidden without a third direction"
expect_hidden "Panel Rectangular Pattern input distance3" "Spacing 3 hidden without a third direction"
ui_step "the direction input"              ui_click_logged "Panel Rectangular Pattern input direction1"
# The origin's axes in the view as its light bulb shows them (mitcad#1030).
ui_show_origin
ui_step "pick the X axis"                  ui_click_logged "Datum x"
ui_expect_new "Rectangular Pattern Direction 1: 1 axis [axis x]" "X picked"
ui_step "quantity 2"                       ui_type_in "Panel Rectangular Pattern input quantity1" "2"
ui_step "spacing 30"                       ui_type_in "Panel Rectangular Pattern input distance1" "30"
ui_mark
ui_step "the second direction input"       ui_click_logged "Panel Rectangular Pattern input direction2"
ui_step "pick the Y axis"                  ui_click_logged "Datum y"
ui_expect_new "Rectangular Pattern Direction 2: 1 axis [axis y]" "Y picked"
ui_step "quantity 3"                       ui_type_in "Panel Rectangular Pattern input quantity2" "3"
ui_mark
ui_step "spacing 30"                       ui_type_in "Panel Rectangular Pattern input distance2" "30"
ui_expect_new "Preview Rectangular Pattern elements: 6" "2 x 3 elements in the preview" 30

echo "--- Direction 3 along Z: a 2 x 3 x 4 grid"
ui_mark
ui_step "the third direction input"        ui_click_logged "Panel Rectangular Pattern input direction3"
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_new "Rectangular Pattern Direction 3: 1 axis [axis z]" "Z picked"
expect_shown "Panel Rectangular Pattern input quantity3" "Quantity 3 shown"
expect_shown "Panel Rectangular Pattern input distance3" "Spacing 3 shown"
ui_expect_new "Manipulator Rectangular Pattern distance3 at" "Spacing 3 has an arrow" 30
ui_expect_new "Arrow direction Rectangular Pattern distance3: 0.000,0.000,1.000" "the arrow points along Z" 30
ui_step "quantity 4"                       ui_type_in "Panel Rectangular Pattern input quantity3" "4"
ui_mark
ui_step "spacing 30"                       ui_type_in "Panel Rectangular Pattern input distance3" "30"
ui_expect_new "Rectangular Pattern Spacing 3: 30 = 30 mm" "30 mm between the layers"
ui_expect_new "Preview Rectangular Pattern elements: 24" "2 x 3 x 4 elements in the preview" 30
ui_expect_new "Preview Rectangular Pattern: ok" "previewed" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added rectangular pattern of 1 bodies: direction 1 one direction, quantity 2, spacing 30 mm; direction 2 one direction, quantity 3, spacing 30 mm; direction 3 one direction, quantity 4, spacing 30 mm" \
  "the definition logged" 30
ui_expect_new "Added Rectangular Pattern as F3" "RectangularPattern1 added" 30

echo "--- The saved copies' places"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved " "saved" 30
# Element e = i + 2 j + 6 k is body F3.b(e-1), its centre (10, 10, 5) +
# 30 (i, j, k): element 1 (1, 0, 0), 6 (0, 0, 1), 5 (1, 2, 0) and 23
# (1, 2, 3).
cat > "$WORK/check.json" << 'EOF'
[
  {"expect": {"bodies": 24}},
  {"expect": {"body": "F3.b0", "center": [40, 10, 5]}},
  {"expect": {"body": "F3.b5", "center": [10, 10, 35]}},
  {"expect": {"body": "F3.b4", "center": [40, 70, 5]}},
  {"expect": {"body": "F3.b22", "center": [40, 70, 95]}}
]
EOF
"$CLI" run "$WORK/check.json" --open "$WORK/block.mitcad" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved design"; }
echo "ok   stored: 24 bodies, the copies along X, Y and Z"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI pattern grid test"
