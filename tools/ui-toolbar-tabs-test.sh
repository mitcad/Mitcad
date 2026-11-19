#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/CommandLayout.cpp
# check-all sources: app/framework/CommandLayout.hpp app/framework/TitleBar.cpp app/resources/layout/default.json
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/toolbar-tabs.mitcad-ui
# The tabs' menu (mitcad#1975) through the real UI, the shared input script
# tools/ui-scripts/toolbar-tabs.mitcad-ui (also run by
# tools/ui-windows-toolbar-tabs-test.ps1 and tools/ui-macos-toolbar-tabs-test.sh),
# in the Floating and the Docked layout: Surface hidden from a right-click
# on a tab and shown again from the + after the last tab, Customize
# Toolbar… giving Sweep a button, Reset Toolbar from a command row's menu
# taking it away.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-toolbar-tabs-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/toolbar-tabs.mitcad-ui
ui_start_display

# run_layout <floating|docked>: the shared script in that layout.
run_layout() {
  ui_start_app --ui=qt --no-mcp "--chrome=$1"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "toolbar tabs driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the tabs menu ($1)"
  ui_expect_log 'Ribbon tab SURFACE hidden' "Surface hidden ($1)"
  ui_expect_log 'Ribbon tab SURFACE shown' "Surface shown again ($1)"
  ui_expect_log 'Customize Toolbar SOLID: solid.sweep on' "Sweep's button from Customize Toolbar ($1)"
  ui_expect_log 'Toolbar reset: SOLID' "the Solid tab reset ($1)"
}

run_layout floating
run_layout docked
ui_finish 'UI Toolbar tabs'
