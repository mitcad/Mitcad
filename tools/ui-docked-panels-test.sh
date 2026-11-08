#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/projects/MainWindowProjectsPanel.cpp app/browser/MainWindowConfigurationPanel.cpp
# check-all sources: app/agent/MainWindowChat.cpp app/framework/WindowStyle.cpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverDocks.cpp tools/ui-scripts/docked-panels.mitcad-ui
# The Docked layout's panels hidden and shown again (mitcad#1455) through
# the real UI, the shared input script tools/ui-scripts/docked-panels.mitcad-ui
# (also run by tools/ui-windows-docked-panels-test.ps1):
#   - The Command dock hidden from the window's menu of its panels (a
#     right-click on its title row) and with its close button comes back
#     with View > Command each time (nothing brought it back before).
#   - The Projects dock likewise with View > Projects.
#   - The window's menu of its panels offers no Configuration (that dock
#     shows while the design has options and has no View entry).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-docked-panels-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/docked-panels.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=docked
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail 'the docked panels driver timed out'
wait "$UI_RUNNER" ||
  ui_fail "the docked panels driver failed: $(grep -E 'TestDriver' "$UI_LOG" | tail -8)"
ui_expect_log 'TestDriver: popup entries: Browser | Timeline | Projects | Command' \
  "the window's menu of its panels offers no Configuration"
ui_expect_log 'TestDriver: dock commandDock hidden, View/Command unchecked' 'the Command dock hidden'
ui_expect_log 'TestDriver: dock projectsDock hidden, View/Projects unchecked' 'the Projects dock hidden'
ui_expect_log 'TestDriver: done' 'the Command and Projects docks come back with the View menu'
grep -E 'TestDriver: (popup entries|dock) ' "$UI_LOG" | sed 's/^/     /'
ui_finish 'UI Docked panels test'
