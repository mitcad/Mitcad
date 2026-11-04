#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/agent/MainWindowChat.cpp app/framework/Ribbon.cpp app/framework/Ribbon.hpp
# check-all sources: app/framework/WindowStyle.cpp app/framework/WindowStyle.hpp app/framework/Theme.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/docked-toolbar-colors.mitcad-ui
# The Docked toolbar's colours in the dark and the light theme (GitHub #127,
# mitcad#1245) through the real UI, the shared input script
# tools/ui-scripts/docked-toolbar-colors.mitcad-ui (also run by
# tools/ui-windows-docked-toolbar-colors-test.ps1): the Classic ribbon's
# body and the bar's Undo, Redo, separator and Chat buttons show the
# toolbar's surface (the "panels" colour of "Window style:") as the window
# draws them, in the dark theme, in the light one after a switch while
# Mitcad runs, and in the dark one again.
# Qt's GTK platform theme gives the palettes (QT_QPA_PLATFORMTHEME=gtk3):
# the generic one has no dark scheme.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-docked-toolbar-colors-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/docked-toolbar-colors.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail 'the toolbar colours driver timed out'
wait "$UI_RUNNER" ||
  ui_fail "the toolbar colours driver failed: $(grep -E 'TestDriver|Window style' "$UI_LOG" | tail -8)"
ui_expect_log 'TestDriver: widget background ribbonPageScroll' "the ribbon's body is logged"
ui_expect_log 'TestDriver: done' "the ribbon's body and the bar's buttons on the toolbar's surface in both themes"
grep 'TestDriver: widget background' "$UI_LOG" | sed 's/^/     /'
ui_finish 'UI Docked toolbar colours test'
