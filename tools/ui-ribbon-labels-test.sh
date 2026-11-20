#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/RibbonLabels.cpp
# check-all sources: app/framework/RibbonLabels.hpp app/framework/RibbonFit.cpp app/view/ViewController.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/ui-scripts/ribbon-labels.mitcad-ui
# check-all sources: app/main.cpp
# Preferences › Display › Toolbar labels (mitcad#1936, GitHub
# mitcad/Mitcad#356) through the real UI, the shared input script
# tools/ui-scripts/ribbon-labels.mitcad-ui (also run by
# tools/ui-windows-ribbon-labels-test.ps1 and
# tools/ui-macos-ribbon-labels-test.sh), in the Floating and the Docked
# layout: Labels under icons by default in both (the toolbar panel,
# mitcad#1990, the Docked layout's too since mitcad#2082); Labels under
# icons makes the rows higher
# and puts Sketch, Extrude, Revolve and Hole under the SOLID tab's Create
# buttons, Create and Modify stay whole in a wide window and the groups
# fold from the right in a narrow one; Icons only again.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-ribbon-labels-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/ribbon-labels.mitcad-ui
ui_start_display

# run_layout <floating|docked>: the shared script in that layout.
run_layout() {
  # The option's index without a choice: Labels under icons in both
  # layouts (the toolbar panel's, mitcad#2082).
  export MITCAD_TEST_LABELS_DEFAULT=1
  # Without the choice the run before saved (its last step chooses Icons
  # only), so that the default shows.
  [ -f "$UI_CONFIG/Mitcad/Mitcad.conf" ] && sed -i '/^ribbonLabels=/d' "$UI_CONFIG/Mitcad/Mitcad.conf"
  ui_start_app --ui=qt --no-mcp "--chrome=$1"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "ribbon labels driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the ribbon's labels ($1)"
  ui_expect_log 'Ribbon group SOLID/CREATE labels: Sketch, Extrude, Revolve, Hole' "the Create group's labels ($1)"
}

run_layout floating
run_layout docked
ui_finish 'UI Toolbar labels'
