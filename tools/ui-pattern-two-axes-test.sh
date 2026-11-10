#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/pattern.rs tools/cli
# Circular Pattern's Axis 2 (mitcad#1588) through the real UI, as
# tools/ui-windows-pattern-two-axes-test.ps1 does in the Windows VM:
#   - on a 20 x 20 x 10 block at the origin (Body1, F2.b0; its centre
#     (10, 10, 5)): Circular Pattern of Body1 about X, 3 over 180 degrees,
#     Symmetric; Axis 2's rows hidden until its axis is picked, then about
#     Z, 4 over 360 degrees; the preview's 12 elements; OK: the added line
#     names both axes;
#   - saved; mitcad-cli reads 12 bodies and the copies' centres, element
#     e = k1 + 3 k2 (k1 about X: 0, 90 and -90 degrees; k2 about Z) body
#     F3.b(e-1); opened again, the pattern's panel shows Axis 2 with its
#     rows;
#   - the cubic lattice of tools/cli/tests/f4_pattern_lattice.json (one
#     revolve with the centre ball, one pipe, one pattern about X and Z, a
#     fillet at every joint): every feature opens in its panel, previews
#     and is left as it was.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pattern-two-axes-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
LATTICE="$(cd "$(dirname "$0")" && pwd)/cli/tests/f4_pattern_lattice.json"
WORK=$(mktemp -d /tmp/mitcad-ui-pattern-two-axes.XXXXXX)
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
"$CLI" run "$LATTICE" --save "$WORK/lattice.mitcad" > "$WORK/lattice.log" 2>&1 ||
  { cat "$WORK/lattice.log"; ui_fail "mitcad-cli (the lattice)"; }

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

# edit_timeline "Timeline <name>": the cell clicked once and the layout let
# settle before the double-click (as tools/ui-pattern-options-test.sh).
edit_timeline() {
  ui_click_logged "$1"
  ui_wait_idle
  sleep 0.5
  ui_sync
  ui_double_click_logged "$1"
}

# opens_in_panel NAME UID COMMAND: the timeline's NAME opens in COMMAND's
# panel for UID, and Esc leaves it as it was.
opens_in_panel() {
  ui_mark
  ui_step "edit $1"                        edit_timeline "Timeline $1"
  ui_expect_new "Editing $2 with $3" "$1 opens in $3"
  ui_wait_idle
  ui_mark
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command $3 cancelled" "$1 left as it was"
}

ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- About X, 3 over 180 degrees, symmetric"
ui_mark
ui_step "circular pattern (search)"        ui_command "Circular Pattern"
ui_expect_new "Command Circular Pattern started" "Circular Pattern started"
ui_expect_new "Panel Circular Pattern checks: symmetric=off, symmetric2=off" "Symmetric 2 among the checks"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_expect_new "Circular Pattern Objects: 1 body [body F2.b0]" "the block picked"
expect_hidden "Panel Circular Pattern input quantity2" "Quantity 2 hidden without a second axis"
expect_hidden "Panel Circular Pattern input angle2" "Total Angle 2 hidden without a second axis"
expect_hidden "Panel Circular Pattern input symmetric2" "Symmetric 2 hidden without a second axis"
ui_step "the axis input"                   ui_click_logged "Panel Circular Pattern input axis"
# The origin's axes in the view as its light bulb shows them (mitcad#1030).
ui_show_origin
ui_step "pick the X axis"                  ui_click_logged "Datum x"
ui_expect_new "Circular Pattern Axis: 1 axis [axis x]" "X picked"
ui_step "quantity 3"                       ui_type_in "Panel Circular Pattern input quantity" "3"
ui_step "angle 180"                        ui_type_in "Panel Circular Pattern input angle" "180"
ui_expect_new "Circular Pattern Total Angle: 180 = 180 deg" "half a turn"
ui_mark
ui_step "Symmetric"                        ui_click_logged "Panel Circular Pattern input symmetric"
ui_expect_new "Circular Pattern: Symmetric = on" "symmetric checked"
ui_expect_new "Preview Circular Pattern elements: 3" "3 elements in the preview" 30

echo "--- Axis 2 about Z, 4 over 360 degrees"
ui_mark
ui_step "the second axis input"            ui_click_logged "Panel Circular Pattern input axis2"
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_new "Circular Pattern Axis 2: 1 axis [axis z]" "Z picked as Axis 2"
expect_shown "Panel Circular Pattern input quantity2" "Quantity 2 shown"
expect_shown "Panel Circular Pattern input angle2" "Total Angle 2 shown"
expect_shown "Panel Circular Pattern input symmetric2" "Symmetric 2 shown"
ui_expect_new "Manipulator Circular Pattern angle2 at" "Total Angle 2 has its ring" 30
ui_mark
ui_step "quantity 4"                       ui_type_in "Panel Circular Pattern input quantity2" "4"
ui_expect_new "Circular Pattern Quantity 2: 4" "four about Z"
ui_expect_new "Preview Circular Pattern elements: 12" "3 x 4 elements in the preview" 30
ui_expect_new "Preview Circular Pattern: ok" "previewed" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added circular pattern of 1 bodies: symmetric, quantity 3, angle 180 deg; axis 2 one direction, quantity 4, angle 360 deg" \
  "the definition logged" 30
ui_expect_new "Added Circular Pattern as F3" "CircularPattern1 added" 30

echo "--- The saved copies' places"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved " "saved" 30
# Element e = k1 + 3 k2 is body F3.b(e-1), the block's centre (10, 10, 5)
# a quarter turn about X to (10, -5, 10), back to (10, 5, -10); element 3 a
# quarter turn about Z, (-10, 10, 5); 4 both, (5, 10, 10); 11 back about X,
# then three quarters about Z, (5, -10, -10).
cat > "$WORK/check.json" << 'EOF'
[
  {"expect": {"bodies": 12}},
  {"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {
    "type": "circular_pattern", "def": {"symmetric": true, "axis2": {"quantity": "d6"}}}}},
  {"expect": {"body": "F3.b0", "center": [10, -5, 10]}},
  {"expect": {"body": "F3.b1", "center": [10, 5, -10]}},
  {"expect": {"body": "F3.b2", "center": [-10, 10, 5]}},
  {"expect": {"body": "F3.b3", "center": [5, 10, 10]}},
  {"expect": {"body": "F3.b10", "center": [5, -10, -10]}}
]
EOF
"$CLI" run "$WORK/check.json" --open "$WORK/block.mitcad" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved design"; }
echo "ok   stored: 12 bodies, the copies about X and Z"

echo "--- Opened again: the panel shows Axis 2"
ui_stop_app
ui_start_app --open "$WORK/block.mitcad"
ui_mark
ui_step "edit CircularPattern1"            edit_timeline "Timeline CircularPattern1"
ui_expect_new "Editing F3 with Circular Pattern" "editing the pattern"
ui_expect_new "Panel Circular Pattern checks: symmetric=on, symmetric2=off" "Symmetric checked, Symmetric 2 not"
expect_shown "Panel Circular Pattern input quantity2" "Quantity 2 shown for the stored second axis"
ui_expect_new "Preview Circular Pattern elements: 12" "the stored 12 elements" 30
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Circular Pattern cancelled" "left as it was"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- The cubic lattice: every feature opens in its panel"
ui_stop_app
ui_start_app --open "$WORK/lattice.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F3.b0" "the lattice shown"
opens_in_panel Plane1 F1 "Plane at Angle"
opens_in_panel Revolve1 F3 Revolve
opens_in_panel Pipe1 F4 Pipe
ui_mark
ui_step "edit CircularPattern1"            edit_timeline "Timeline CircularPattern1"
ui_expect_new "Editing F5 with Circular Pattern" "CircularPattern1 opens in Circular Pattern"
ui_expect_new "Panel Circular Pattern checks: symmetric=on, symmetric2=off" "the lattice's pattern: symmetric about X"
ui_expect_new "Preview Circular Pattern elements: 12" "the lattice's pattern previews its 12 elements" 60
ui_wait_idle
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Circular Pattern cancelled" "CircularPattern1 left as it was"
opens_in_panel Fillet1 F6 Fillet
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI pattern two axes test"
