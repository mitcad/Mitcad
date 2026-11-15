#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/Navigation.hpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverNavigation.cpp
# check-all sources: tools/ui-scripts/navigation-drag-zoom.mitcad-ui
# A drag zoom follows the pointer's vertical motion (mitcad#1867, GitHub
# mitcad/Mitcad#330) through the real view, the shared input script
# tools/ui-scripts/navigation-drag-zoom.mitcad-ui (also run by
# tools/ui-windows-navigation-drag-zoom-test.ps1 and
# tools/ui-macos-navigation-drag-zoom-test.sh) on the demo block, in the
# Floating and the Docked layout: with the middle-orbit scheme, Shift +
# middle dragged straight up comes closer, straight down goes away,
# sideways alone keeps the zoom, and up with a sideways wobble comes closer
# at every step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-navigation-drag-zoom-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/navigation-drag-zoom.mitcad-ui
ui_start_display
for chrome in floating docked; do
  echo "=== $chrome"
  ui_start_app --ui=qt --no-mcp --demo --chrome="$chrome"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$chrome: drag zoom driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { grep -E 'TestDriver|Drag zoom' "$UI_LOG" | tail -12 >&2; ui_fail "$chrome: drag zoom driver failed"; exit 1; }
  ui_expect_log 'Drag zoom: straight up came closer, straight down went away, sideways kept the zoom, an upward drag with a sideways wobble came closer at every step (Shift + middle)' \
    "$chrome: Shift + middle zooms by the vertical motion"
  ui_expect_log 'TestDriver: done' "$chrome: drag zoom completed"
done
ui_finish 'UI navigation drag zoom'
