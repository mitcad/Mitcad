#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/framework/EnterRepeat.cpp
# check-all sources: app/framework/EnterRepeat.hpp app/framework/CommandSession.cpp tools/cli
# Enter repeats the last command (mitcad#1620) through the real UI, in both
# layouts, on a design mitcad-cli makes: a 30 x 20 mm rectangle in an
# unused sketch (Sketch1, F1), Extrude's profile when it starts with
# nothing selected.
#   1. E starts Extrude with the rectangle; 10 mm typed in its Distance
#      field and Enter there: Enter in a panel still means OK, the extrusion
#      is added.
#   2. Enter with no command running and the 3D view focused: Repeat
#      Extrude, logged as Repeat; it starts with nothing (the sketch is
#      used now) and Esc cancels it as any command.
#   3. A rename in the browser (a double-click on Body1): Enter there ends
#      the rename and repeats nothing.
# The Windows build runs the same code; its UI tests of Enter in panels
# (Enter as OK) are unchanged.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-enter-repeat-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-enter-repeat.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/sketch.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20}
]
EOF
"$CLI" run "$WORK/sketch.json" --save "$WORK/sketch.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# no_new "text" description: no such line since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

# repeat_steps layout: the three steps above in a window of that layout.
repeat_steps() {
  local layout=$1
  echo "=== $layout"
  ui_start_app --chrome="$layout" --open "$WORK/sketch.mitcad"
  ui_step "fit (F6)"                       ui_key F6

  echo "--- 1. Extrude by E, Enter in its panel is OK"
  ui_mark
  ui_step "extrude (E), nothing selected" ui_key e
  ui_expect_new "Command Extrude started" "$layout: Extrude started"
  ui_expect_new "Extrude Profiles: 1 profile" "$layout: the rectangle is the profile"
  ui_step "distance 10"                    ui_type_in "Panel Extrude input distance" "10"
  ui_expect_new "Preview Extrude: ok" "$layout: previewed"
  ui_mark
  ui_step "OK (Enter in the field)"        ui_key Return
  ui_expect_new "Added extrude (New Body), distance 10" "$layout: Enter in the panel is OK"
  no_new "Repeat " "$layout: Enter in the panel repeats nothing"

  echo "--- 2. Enter with no command: Repeat Extrude; Esc cancels it"
  ui_sync
  ui_mark
  ui_step "Enter (no command running)"     ui_key Return
  ui_expect_new "Repeat Extrude (Enter)" "$layout: Enter repeats the last command"
  ui_expect_new "Command Extrude started" "$layout: Extrude started again"
  ui_mark
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Extrude cancelled" "$layout: Esc cancels the repeated command"

  echo "--- 3. Enter in a rename ends the rename only"
  ui_mark
  ui_step "double-click Body1"             ui_double_click_logged "Browser Root/Bodies/Body1"
  ui_expect_new "Renaming Root/Bodies/Body1" "$layout: Body1 renamed in place"
  ui_step "type Plate"                     xdotool type --delay 40 "Plate"
  ui_mark
  ui_step "Enter (in the rename)"          ui_key Return
  ui_expect_new "Renamed Body1 to Plate" "$layout: the rename ended"
  ui_sync
  no_new "Repeat Extrude" "$layout: Enter in the rename repeats nothing"
  no_new "Command Extrude started" "$layout: no Extrude started"
  ui_stop_app
}

ui_start_display
repeat_steps floating
repeat_steps docked
ui_finish "Enter repeats the last command"
