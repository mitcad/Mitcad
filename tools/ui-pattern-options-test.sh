#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/pattern.rs tools/cli
# The patterns' Direction Type and Distance Type (mitcad#545) through the
# real UI, as tools/ui-windows-pattern-options-test.ps1 does in the Windows
# VM, on a 10 mm cube with its corner at (20, 20, 0) (Body1, F2.b0; its
# centre (25, 25, 5)):
#   - Rectangular Pattern from its defaults (spacing, one direction):
#     Distance Type Extent (the length rows say Extent), direction 1 along X
#     Symmetric (Flip hidden), 3 over 40 mm; direction 2 along Y, 3 over
#     100 mm; the added line carries the options;
#   - Circular Pattern about Z, 3 in 90 degrees, Symmetric;
#   - saved; mitcad-cli reads the stored definitions and the copies'
#     centres; opened again, the panels show the stored options.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pattern-options-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-pattern-options.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/cube.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [20, 20], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/cube.json" --save "$WORK/cube.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_hidden "Panel <command> input <id>" description: the input's last
# logged place says it is hidden.
expect_hidden() {
  for _ in $(seq 1 30); do
    grep -F "$1 at " "$UI_LOG" | tail -1 | grep -q '(hidden)$' && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: the last place of '$1' is not hidden"
}

# show_axes description: the view zoomed out about the cube, a wheel step
# at a time, until the origin axes' pick places lie in the view (30 px in
# from its edges), as tools/ui-windows-pattern-options-test.ps1 does. The
# view fitted to the 10 mm cube at (20, 20, 0) shows the origin's axes
# beyond the window (the X axis' place at y 1066 in an 800 px window, the
# origin's left of it); the datums log their places again whenever the
# camera rests, also after a zoom (mitcad#546).
show_axes() {
  local area vx vy vw vh axis line px py inside count
  # The axis input shows the origin only as its light bulb does (mitcad#1030).
  ui_show_origin
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  [ -n "$area" ] || ui_fail "$1: the app did not log its view area"
  read -r _ _ vx vy vw vh <<< "$area"
  for _ in $(seq 0 24); do
    inside=1
    for axis in x y z; do
      line=$(grep "^Datum $axis at " "$UI_LOG" | tail -1)
      if [[ $line =~ \ at\ (-?[0-9]+),(-?[0-9]+)$ ]]; then
        px=${BASH_REMATCH[1]}
        py=${BASH_REMATCH[2]}
        if ((px < vx + 30 || px > vx + vw - 30 || py < vy + 30 || py > vy + vh - 30)); then
          inside=0
        fi
      else
        inside=0
      fi
    done
    if [ "$inside" = 1 ]; then
      echo "ok   $1"
      return
    fi
    count=$(grep -c "^Datum x at " "$UI_LOG")
    xdotool mousemove $(ui_logged_at "Body F2.b0") click 5
    for _ in $(seq 1 20); do
      [ "$(grep -c "^Datum x at " "$UI_LOG")" -gt "$count" ] && break
      sleep 0.2
    done
    ui_sync
  done
  ui_fail "$1: the origin axes' places stayed outside the view"
}

# edit_timeline "Timeline <name>": the cell clicked once and the layout let
# settle before the double-click, as the Windows test's Edit-Timeline. In
# the docked layout the click selects the pattern's bodies, the Browser
# opens its Bodies folder for them and the window grows (the timeline from
# y 749 to 956), so a double-click's second press landed beside the cell
# and selected nothing.
edit_timeline() {
  ui_click_logged "$1"
  ui_wait_idle
  sleep 0.5
  ui_sync
  ui_double_click_logged "$1"
}

ui_start_display
ui_start_app --open "$WORK/cube.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- A rectangular pattern: symmetric along X, by extent"
ui_mark
ui_step "rectangular pattern (search)"     ui_command "Rectangular Pattern"
ui_expect_new "Panel Rectangular Pattern choices: object_type=bodies, compute=adjust, distance_type=spacing, direction_type1=one, direction_type2=one" \
  "the defaults: spacing, one direction"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_expect_new "Rectangular Pattern Objects: 1 body [body F2.b0]" "the cube picked"
ui_step "the direction input"              ui_click_logged "Panel Rectangular Pattern input direction1"
show_axes "the origin axes in the view"
ui_step "pick the X axis"                  ui_click_logged "Datum x"
ui_expect_new "Rectangular Pattern Direction 1: 1 axis [axis x]" "X picked"
ui_mark
ui_step "Distance Type: Extent"            ui_choose "Panel Rectangular Pattern input distance_type" 1
ui_expect_new "Rectangular Pattern: Distance Type = Extent" "extent chosen"
ui_expect_new "Panel Rectangular Pattern label distance1: Extent" "the length row says Extent"
ui_expect_new "Panel Rectangular Pattern label distance2: Extent 2" "the second one too"
ui_expect_new "Preview Rectangular Pattern: ok" "the preview follows" 30
ui_mark
ui_step "Direction Type: Symmetric"        ui_choose "Panel Rectangular Pattern input direction_type1" 1
ui_expect_new "Rectangular Pattern: Direction Type = Symmetric" "symmetric chosen"
ui_expect_new "Preview Rectangular Pattern: ok" "the preview follows" 30
expect_hidden "Panel Rectangular Pattern input flip1" "Flip hidden while symmetric"
ui_mark
ui_step "extent 40"                        ui_type_in "Panel Rectangular Pattern input distance1" "40"
ui_expect_new "Rectangular Pattern Extent: 40 = 40 mm" "40 mm from the first to the last"
ui_step "the second direction input"       ui_click_logged "Panel Rectangular Pattern input direction2"
show_axes "the origin axes in the view"
ui_step "pick the Y axis"                  ui_click_logged "Datum y"
ui_expect_new "Rectangular Pattern Direction 2: 1 axis [axis y]" "Y picked"
ui_step "quantity 3"                       ui_type_in "Panel Rectangular Pattern input quantity2" "3"
ui_mark
ui_step "extent 100"                       ui_type_in "Panel Rectangular Pattern input distance2" "100"
ui_expect_new "Rectangular Pattern Extent 2: 100 = 100 mm" "100 mm from the first to the last"
ui_expect_new "Preview Rectangular Pattern: ok" "previewed" 30
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added rectangular pattern of 1 bodies: direction 1 symmetric, quantity 3, extent 40 mm; direction 2 one direction, quantity 3, extent 100 mm" \
  "the definition logged" 30
ui_expect_new "Added Rectangular Pattern as F3" "RectangularPattern1 added" 30

echo "--- A circular pattern: symmetric about Z in 90 degrees"
ui_mark
ui_step "circular pattern (search)"        ui_command "Circular Pattern"
ui_expect_new "Panel Circular Pattern checks: symmetric=off" "not symmetric by default"
ui_step "pick Body1"                       ui_click_logged "Body F2.b0"
ui_step "the axis input"                   ui_click_logged "Panel Circular Pattern input axis"
show_axes "the origin axes in the view"
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_new "Circular Pattern Axis: 1 axis [axis z]" "Z picked"
ui_step "quantity 3"                       ui_type_in "Panel Circular Pattern input quantity" "3"
ui_step "angle 90"                         ui_type_in "Panel Circular Pattern input angle" "90"
ui_mark
ui_step "Symmetric"                        ui_click_logged "Panel Circular Pattern input symmetric"
ui_expect_new "Circular Pattern: Symmetric = on" "symmetric checked"
ui_expect_new "Preview Circular Pattern: ok" "the preview follows" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added circular pattern of 1 bodies: symmetric, quantity 3, angle 90 deg" "the definition logged" 30
ui_expect_new "Added Circular Pattern as F4" "CircularPattern1 added" 30

echo "--- The stored definitions and the copies' places"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved " "saved" 30
# Element e of the rectangular pattern is body F3.b(e-1), e = k1 + 3 k2:
# along X at 0, +20, -20 (symmetric), along Y at 0, +50, +100. The circular
# pattern's elements at +45 and -45 degrees.
cat > "$WORK/check.json" << 'EOF'
[
  {"expect": {"bodies": 11}},
  {"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {
    "type": "rectangular_pattern", "def": {"direction1": {"symmetric": true}, "distance_type": "extent"}}}},
  {"expect": {"query": {"query": "feature", "uid": "F4"}, "result": {
    "type": "circular_pattern", "def": {"symmetric": true}}}},
  {"expect": {"body": "F3.b0", "center": [45, 25, 5]}},
  {"expect": {"body": "F3.b1", "center": [5, 25, 5]}},
  {"expect": {"body": "F3.b2", "center": [25, 75, 5]}},
  {"expect": {"body": "F3.b7", "center": [5, 125, 5]}},
  {"expect": {"body": "F4.b0", "center": [0, 35.35533905932738, 5]}},
  {"expect": {"body": "F4.b1", "center": [35.35533905932738, 0, 5]}}
]
EOF
"$CLI" run "$WORK/check.json" --open "$WORK/cube.mitcad" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved design"; }
echo "ok   stored: direction 1 symmetric, extent; the circular pattern symmetric; the copies on both sides"

echo "--- Opened again: the panels show the stored options"
ui_stop_app
ui_start_app --open "$WORK/cube.mitcad"
ui_mark
ui_step "edit RectangularPattern1"         edit_timeline "Timeline RectangularPattern1"
ui_expect_new "Editing F3 with Rectangular Pattern" "editing the pattern"
ui_expect_new "Panel Rectangular Pattern choices: object_type=bodies, compute=adjust, distance_type=extent, direction_type1=symmetric, direction_type2=one" \
  "Extent and Symmetric shown"
ui_expect_new "Panel Rectangular Pattern label distance1: Extent" "the length row says Extent"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Rectangular Pattern cancelled" "left as it was"
ui_mark
ui_step "edit CircularPattern1"            edit_timeline "Timeline CircularPattern1"
ui_expect_new "Editing F4 with Circular Pattern" "editing the circular pattern"
ui_expect_new "Panel Circular Pattern checks: symmetric=on" "Symmetric checked"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Circular Pattern cancelled" "left as it was"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI pattern options test"
