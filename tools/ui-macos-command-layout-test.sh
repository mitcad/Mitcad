#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/CommandLayout.cpp
# check-all sources: app/framework/CommandLayout.hpp app/view/ViewController.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/command-layout.mitcad-ui
# check-all sources: tools/ui-scripts/command-layout-restart.mitcad-ui
# The command layout's user layer (mitcad#1951, mitcad#1991) on macOS,
# through the test driver (tools/ui-scripts/command-layout.mitcad-ui, then
# after a restart command-layout-restart.mitcad-ui, the same scripts as the
# Linux and Windows tests'), in the Floating and the Docked layout: Sweep
# moved from Create to Modify, then shown in both groups; Revolve's button
# removed by its right-click and Inspect moved after Modify; after the
# restart both kept, Show on Toolbar from Create's caption brings the button
# back, Remove from Toolbar takes it away again, and Settings › Restore
# Default Layout puts everything back.
# Usage: tools/ui-macos-command-layout-test.sh <app> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-command-layout}"
UI_TIMEOUT=180
# The runs share the settings (MITCAD_SETTINGS_DIR of ui_init).
for layout in floating docked; do
  MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/command-layout.mitcad-ui \
    ui_run_app "command-layout-$layout" --ui=qt --no-recovery --no-mcp "--chrome=$layout"
  ui_expect_log 'Layout: moved solid.sweep from SOLID/CREATE to SOLID/MODIFY' "Sweep moved ($layout)"
  ui_expect_log 'Layout: removed the button of solid.revolve from SOLID/CREATE' "Revolve's button removed ($layout)"
  ui_expect_log 'Layout: moved the group SOLID/INSPECT: CREATE, MODIFY, INSPECT, ASSEMBLE' "Inspect moved after Modify ($layout)"
  ui_expect_log 'TestDriver: done' "the command layout ($layout)"
  ui_expect_no_log '^TestDriver: FAIL' "no failed driver assertion ($layout)"
  MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/command-layout-restart.mitcad-ui \
    ui_run_app "command-layout-restart-$layout" --ui=qt --no-recovery --no-mcp "--chrome=$layout"
  ui_expect_log 'Ribbon groups of SOLID: CREATE, MODIFY, INSPECT, ASSEMBLE' "the order kept after a restart ($layout)"
  ui_expect_log 'Layout: showed the button of solid.revolve in SOLID/CREATE again' "Show on Toolbar ($layout)"
  ui_expect_log 'Preferences: layout restored to the default' "the default restored ($layout)"
  ui_expect_log 'TestDriver: done' "the command layout after a restart ($layout)"
  ui_expect_no_log '^TestDriver: FAIL' "no failed driver assertion after a restart ($layout)"
done
[ "$UI_FAILURES" = 0 ] || exit 1
