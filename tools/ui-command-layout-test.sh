#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/CommandLayout.cpp
# check-all sources: app/framework/CommandLayout.hpp app/resources/layout/default.json app/view/ViewController.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/ui-scripts/command-layout.mitcad-ui
# check-all sources: tools/ui-scripts/command-layout-restart.mitcad-ui
# The command layout's user layer (mitcad#1951, mitcad#1991) through the
# real UI, the shared input scripts tools/ui-scripts/command-layout.mitcad-ui
# and command-layout-restart.mitcad-ui (also run by
# tools/ui-windows-command-layout-test.ps1 and
# tools/ui-macos-command-layout-test.sh), in the Floating and the Docked
# layout: Sweep moved from Create to Modify, then shown in both groups;
# Revolve's button removed by its right-click (still in Create's menu) and
# Inspect moved after Modify; after a restart both kept, Show on Toolbar
# from Create's caption brings the button back, Remove from Toolbar from
# Revolve's row takes it away, and Preferences › Restore Default Layout
# puts everything back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-command-layout-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
ui_start_display

# run_script <script> <floating|docked>: one of the shared scripts in that
# layout, on the settings the runs share.
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp "--chrome=$2"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "command layout driver timed out ($1, $2)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the command layout ($1, $2)"
}

# run_layout <floating|docked>: both scripts, the second after a restart.
run_layout() {
  run_script command-layout "$1"
  ui_expect_log 'Layout: moved solid.sweep from SOLID/CREATE to SOLID/MODIFY' "Sweep moved ($1)"
  ui_expect_log 'Layout: removed the button of solid.revolve from SOLID/CREATE' "Revolve's button removed ($1)"
  ui_expect_log 'Layout: moved the group SOLID/INSPECT: CREATE, MODIFY, INSPECT, ASSEMBLE' "Inspect moved after Modify ($1)"
  run_script command-layout-restart "$1"
  ui_expect_log 'Ribbon groups of SOLID: CREATE, MODIFY, INSPECT, ASSEMBLE' "the order kept after a restart ($1)"
  ui_expect_log 'Layout: showed the button of solid.revolve in SOLID/CREATE again' "Show on Toolbar ($1)"
  ui_expect_log 'Preferences: layout restored to the default' "the default restored ($1)"
}

run_layout floating
run_layout docked
ui_finish 'UI Command layout'
