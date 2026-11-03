#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPrint.cpp app/icons/print-3d.svg app/icons/print-settings.svg
# check-all sources: app/icons/print-process.svg
# check-all sources: app/icons/print-analysis.svg app/framework/TestDriver.cpp tools/ui-scripts/print-icons.mitcad-ui
# The Make tab's 3D Print group shows four different icons (mitcad#1096,
# mitcad#1135): Send to Slicer the printer (print-3d), Printers… the
# printer with a gear (print-settings), Processes… the printed layers with
# a gear (print-process), Print Analysis its own; checked on
# the commands' actions and their ribbon buttons, in the Floating and the
# Docked layout (tools/ui-scripts/print-icons.mitcad-ui).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
ui_start_display
for chrome in floating docked; do
  MITCAD_TEST_INPUT=$here/ui-scripts/print-icons.mitcad-ui \
    ui_start_app --ui=qt --no-mcp --demo --chrome=$chrome
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the 3D Print icons script ($chrome) timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'TestDriver: done' "the 3D Print icons script passed ($chrome)"
  for pair in "make.print3d print-3d" "make.printers print-settings" "make.processes print-process" \
    "inspect.print_analysis print-analysis"; do
    set -- $pair
    grep -qE "TestDriver: command $1 shows the icon $2 \([1-9][0-9]* buttons\)" "$UI_LOG" ||
      ui_fail "$1 has no ribbon button with the icon $2 ($chrome)"
    echo "ok   $1 shows $2 on its action and ribbon buttons ($chrome)"
  done
done
ui_finish '3D Print icons'
