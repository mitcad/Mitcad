#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/CommandLayout.cpp
# check-all sources: app/framework/CommandLayout.hpp app/framework/TitleBar.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/toolbar-tabs.mitcad-ui
# The tabs' menu (mitcad#1975) on macOS, through the test driver
# (tools/ui-scripts/toolbar-tabs.mitcad-ui, the same script as the Linux
# and Windows tests'), in the Floating and the Docked layout: Surface
# hidden and shown again, Customize Toolbar… and Reset Toolbar.
# Usage: tools/ui-macos-toolbar-tabs-test.sh <app> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-toolbar-tabs}"
UI_TIMEOUT=180
for layout in floating docked; do
  MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/toolbar-tabs.mitcad-ui \
    ui_run_app "toolbar-tabs-$layout" --ui=qt --no-recovery --no-mcp "--chrome=$layout"
  ui_expect_log 'Ribbon tab SURFACE hidden' "Surface hidden ($layout)"
  ui_expect_log 'Ribbon tab SURFACE shown' "Surface shown again ($layout)"
  ui_expect_log 'Customize Toolbar SOLID: solid.sweep on' "Sweep's button ($layout)"
  ui_expect_log 'Toolbar reset: SOLID' "the Solid tab reset ($layout)"
  ui_expect_log 'TestDriver: done' "the tabs menu ($layout)"
  ui_expect_no_log '^TestDriver: FAIL' "no failed driver assertion ($layout)"
done
[ "$UI_FAILURES" = 0 ] || exit 1
