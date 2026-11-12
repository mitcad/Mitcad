#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/WindowPlace.cpp app/framework/WindowPlace.hpp app/WindowManager.cpp
# check-all sources: app/DocumentFrame.cpp app/main.cpp app/framework/FitToScreen.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverWindowFit.cpp
# check-all sources: tools/ui-scripts/window-place-save.mitcad-ui tools/ui-scripts/window-place-restore.mitcad-ui
# check-all sources: tools/ui-scripts/window-place-maximized.mitcad-ui tools/ui-scripts/window-place-on-screen.mitcad-ui
# The design window's size, place and state kept between starts
# (mitcad#1710, GitHub mitcad/Mitcad#283), through starts of MitCad with
# the input scripts tools/ui-scripts/window-place-*.mitcad-ui:
#   1. Nothing kept: the start size; the window resized to 900 x 600 and
#      moved to 70, 50; kept when MitCad ends.
#   2. Started again: the same size and place; maximized, kept so.
#   3. Started again: maximized; made normal, the same size and place; a
#      new window at that size, cascaded.
#   4. A kept place on a monitor no longer connected, one half off the
#      screen with its title bar above the top, and one larger than the
#      screen (written into the settings): the whole window within the
#      primary screen's available area.
# In the Floating layout; 1 and 2 also in the Docked one.
#
# Runs headless on Xvfb (see ui-test-lib.sh). MitCad is started here, not
# with ui_start_app, which moves the window to the screen's corner once it
# shows: the window's place is what this test checks.
# Usage: tools/ui-window-place-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

# run name script layout: one start of MitCad running
# tools/ui-scripts/window-place-<script>.mitcad-ui; the app ends with it.
run() {
  local name=$1
  echo "--- $name"
  export MITCAD_TEST_INPUT=$here/ui-scripts/window-place-$2.mitcad-ui
  "$UI_APP" --no-recovery --ui=qt --no-mcp "--chrome=$3" > "$UI_LOG" 2>&1 &
  UI_RUNNER=$!
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$name: the window place driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$name: the window place driver failed: $(grep -E 'TestDriver|Window place' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$name"
  grep -E '^Window place|^TestDriver: window ' "$UI_LOG" | sed 's/^/     /'
}

# expect_kept text description: the settings file has the line.
expect_kept() {
  grep -qxF -- "$1" "$CONF" || ui_fail "$2: '$1' not in the settings ($(grep -A6 '^\[mainWindow\]' "$CONF" | tr '\n' ' '))"
  echo "ok   $2"
}

# seed geometry description: a kept place written into the settings, on no
# connected screen's area as a whole; MitCad moves it onto the primary one.
seed() {
  mkdir -p "$(dirname "$CONF")"
  printf '[mainWindow]\ngeometry=@Rect(%s)\nscreen=gone-monitor\nmaximized=false\n' "$1" > "$CONF"
  run "$2" on-screen floating
}

ui_start_display

for layout in floating docked; do
  rm -f "$CONF"
  run "$layout: nothing kept, resized and moved" save "$layout"
  ui_expect_log 'Window place saved: ' "$layout: the place kept when MitCad ends"
  expect_kept 'maximized=false' "$layout: kept as a normal window"
  run "$layout: started again at the same size and place" restore "$layout"
  expect_kept 'maximized=true' "$layout: kept as maximized"
done

run "floating: started again maximized, normal at the same place" maximized floating

seed '3000 200 900 600' "a removed monitor: on the primary screen"
seed '-450 -20 900 600' "half off the screen, the title bar above the top: on the screen"
seed '-100 -100 4000 3000' "larger than the screen: within it"

ui_finish 'UI window place test'
