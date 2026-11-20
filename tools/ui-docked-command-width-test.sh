#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandDockWidth.cpp app/framework/CommandDockWidth.hpp
# check-all sources: app/framework/UiSizes.cpp app/framework/UiSizes.hpp app/MainWindow.cpp
# check-all sources: app/sketch/SketchPalette.cpp app/sketch/SketchPalette.hpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/docked-command-width.mitcad-ui
# check-all sources: tools/ui-scripts/docked-command-width-restart.mitcad-ui
# The Docked layout's Command panel at the largest Text size and Icon size
# (mitcad#1825, GitHub mitcad/Mitcad#336) through the real UI, the shared
# input scripts tools/ui-scripts/docked-command-width.mitcad-ui and
# docked-command-width-restart.mitcad-ui (also run by
# tools/ui-windows-docked-command-width-test.ps1), MitCad started Docked
# with appearance/textSize and appearance/iconSize 200:
#   1. The panel is at least 600 px wide (300 px times 200 %); in a sketch
#      every row of the Sketch Palette lies whole within it and none is
#      scrolled sideways (its widgets' geometry, "Palette rows cut: none").
#   2. Its edge dragged wider: the width is remembered
#      (commandPanel/dockWidth), at least 100 px more than before, the rows
#      whole at it.
#   3. Restarted: the panel has that width again, the rows whole.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-docked-command-width-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
# The sizes alone: no factor of the test library's (UI_SCALE) nor one kept
# from another start.
unset QT_SCALE_FACTOR MITCAD_USER_QT_SCALE_FACTOR
export MITCAD_TEST_STEP_MS=100
UI_SCREEN_HEIGHT=1080
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

# run name script: one Docked start of MitCad with the input script; the
# app ends with the script's steps.
run() {
  local name=$1 script=$2
  export MITCAD_TEST_INPUT=$here/ui-scripts/$script
  echo "--- $name"
  ui_start_app --ui=qt --no-mcp --chrome=docked
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$name: the driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$name: the driver failed: $(grep -E 'TestDriver|Command panel|Palette rows cut' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$name: the script's steps"
  grep -E '^Command panel|^Command edge at |^Palette rows cut: |^Palette buttons clipped: ' "$UI_LOG" |
    tail -8 | sed 's/^/     /'
}

ui_start_display
mkdir -p "$(dirname "$CONF")"
printf '[appearance]\ntextSize=200\niconSize=200\n' > "$CONF"

echo "--- 1. and 2. The panel at 200 %, its edge dragged wider"
run "Docked, text and icons 200%" docked-command-width.mitcad-ui
first=$(grep -oE '^Command panel width [0-9]+ ' "$UI_LOG" | head -1 | cut -d" " -f4)
[ -n "$first" ] && [ "$first" -ge 600 ] || ui_fail "the panel is ${first:-?} px wide at 200 %, not at least 600"
echo "ok   the panel opened $first px wide (at least 600)"
width=$(grep -oE '^Command panel: dock width [0-9]+ remembered$' "$UI_LOG" | tail -1 | cut -d" " -f5)
[ -n "$width" ] && [ "$width" -ge $((first + 100)) ] ||
  ui_fail "the dragged panel is ${width:-?} px wide (was $first)"
grep -q "^dockWidth=$width\$" "$CONF" || ui_fail "the width $width is not kept in $CONF"
echo "ok   the dragged panel is $width px wide, remembered"

echo "--- 3. Restarted: the remembered width again"
# Without the docks' kept arrangement (mitcad#2070), which would give the
# panel its width at quit instead: the remembered width's own restore, as at
# a start with nothing kept (tools/ui-dock-state-test.sh tests the other).
sed -i '/^\[dockState\]/,/^\[/{/^\[dockState\]/d;/^\[/!d}' "$CONF"
export MITCAD_TEST_COMMAND_WIDTH=$width
run "Docked, restarted" docked-command-width-restart.mitcad-ui
restored=$(grep -oE '^Command panel width [0-9]+ ' "$UI_LOG" | tail -1 | cut -d" " -f4)
[ -n "$restored" ] && [ "$restored" -ge $((width - 10)) ] && [ "$restored" -le $((width + 10)) ] ||
  ui_fail "the panel is ${restored:-?} px wide after the restart, not $width"
echo "ok   the panel is $restored px wide again ($width remembered)"

ui_finish 'UI Docked Command panel width test'
