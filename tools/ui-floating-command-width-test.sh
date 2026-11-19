#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/FloatingLayout.cpp app/framework/FloatingLayout.hpp
# check-all sources: app/framework/UiSizes.cpp app/framework/UiSizes.hpp app/MainWindow.cpp
# check-all sources: app/sketch/SketchPalette.cpp app/sketch/SketchPalette.hpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/floating-command-width.mitcad-ui
# The Floating layout's Command card at the largest Text size and Icon size
# (mitcad#1846, follow-up of mitcad#1825's Docked panel,
# tools/ui-docked-command-width-test.sh) through the real UI, the shared
# input script tools/ui-scripts/floating-command-width.mitcad-ui (also run
# by tools/ui-windows-floating-command-width-test.ps1 and
# tools/ui-macos-floating-command-width-test.sh), MitCad started Floating
# with appearance/textSize and appearance/iconSize 200:
#   1. With Box's command panel the card is at least 600 px wide (300 px
#      times 200 %).
#   2. In a sketch the card is at least 600 px wide, every row of the Sketch
#      Palette lies whole within it and none is scrolled sideways ("Palette
#      rows cut: none"), no Create or Constraints button is clipped.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-floating-command-width-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
# The sizes alone: no factor of the test library's (UI_SCALE) nor one kept
# from another start.
unset QT_SCALE_FACTOR MITCAD_USER_QT_SCALE_FACTOR
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/floating-command-width.mitcad-ui
UI_SCREEN_HEIGHT=1080
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

ui_start_display
mkdir -p "$(dirname "$CONF")"
# Icons only: the toolbar panel without its labels (mitcad#1990), as the
# script's steps were laid out.
printf '[appearance]\ntextSize=200\niconSize=200\nribbonLabels=icons\n' > "$CONF"

echo "--- Floating, text and icons 200%"
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the driver timed out"
wait "$UI_RUNNER" ||
  ui_fail "the driver failed: $(grep -E 'TestDriver|commandDock|Palette rows cut' "$UI_LOG" | tail -8)"
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "the script's steps"
grep -E '^Palette rows cut: |^Palette buttons clipped: |^Palette Constraints: ' "$UI_LOG" | tail -3 | sed 's/^/     /'

width=$(grep -E '^Floating cards: ' "$UI_LOG" | grep -oE '; commandDock -?[0-9]+,-?[0-9]+ [0-9]+x' | tail -1 |
  grep -oE ' [0-9]+x$' | tr -d ' x')
[ -n "$width" ] && [ "$width" -ge 600 ] || ui_fail "the command card is ${width:-?} px wide at 200 %, not at least 600"
echo "ok   the command card is $width px wide (at least 600)"
grep -qE '^Palette rows cut: none [(]' "$UI_LOG" || ui_fail "a row of the Sketch Palette is cut in the card"
echo "ok   the Sketch Palette's rows are whole in the card"
grep -qE '^Palette buttons clipped: none ' "$UI_LOG" || ui_fail "a button of the Sketch Palette is clipped"
echo "ok   no button of the Sketch Palette is clipped"

ui_finish 'UI Floating Command card width test'
