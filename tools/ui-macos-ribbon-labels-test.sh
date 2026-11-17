#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/RibbonLabels.cpp
# check-all sources: app/framework/RibbonLabels.hpp app/view/ViewController.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/ribbon-labels.mitcad-ui
# Preferences › Display › Toolbar labels (mitcad#1936) on macOS, through the
# test driver (tools/ui-scripts/ribbon-labels.mitcad-ui, the same script as
# the Linux and Windows tests'), in the Floating and the Docked layout:
# Icons only by default; Labels under icons in Settings makes the rows
# higher and puts Sketch, Extrude, Revolve and Hole under the SOLID tab's
# Create buttons, Create and Modify stay whole in a wide window and the
# groups fold from the right in a narrow one; Icons only again.
# Usage: tools/ui-macos-ribbon-labels-test.sh <app> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-ribbon-labels}"
UI_TIMEOUT=180
for layout in floating docked; do
  MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/ribbon-labels.mitcad-ui \
    ui_run_app "ribbon-labels-$layout" --ui=qt --no-recovery --no-mcp "--chrome=$layout"
  ui_expect_log 'Ribbon group SOLID/CREATE labels: Sketch, Extrude, Revolve, Hole' \
    "the Create group's labels ($layout)"
  ui_expect_log 'TestDriver: done' "the ribbon's labels ($layout)"
  ui_expect_no_log '^TestDriver: FAIL' "no failed driver assertion ($layout)"
done
[ "$UI_FAILURES" = 0 ] || exit 1
