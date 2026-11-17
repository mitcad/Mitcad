#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/Navigation.hpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverNavigation.cpp
# check-all sources: tools/ui-scripts/navigation-device-scroll.mitcad-ui
# A touchpad's two-finger scroll under the Trackpad scheme on Linux's xcb
# platform (GitHub mitcad/Mitcad#381) through the real UI, the input script
# tools/ui-scripts/navigation-device-scroll.mitcad-ui on the demo block, in
# the Floating and the Docked layout: libinput's wheel steps (no phase, no
# pixel deltas) from a touchpad device pan both ways, Alt + a vertical move
# orbits vertically although xcb swaps its axes, and a mouse wheel's whole
# and partial notches still zoom.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-navigation-device-scroll-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/navigation-device-scroll.mitcad-ui
ui_start_display
for chrome in floating docked; do
  echo "=== $chrome"
  ui_start_app --ui=qt --no-mcp --demo --chrome="$chrome"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$chrome: device scroll driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$chrome: device scroll driver failed"; exit 1; }
  ui_expect_log "Device scroll: a touchpad's wheel steps panned both ways and Alt orbited vertically" \
    "$chrome: a touchpad's wheel steps pan, and Alt orbits vertically"
  ui_expect_log "Device scroll: a mouse wheel's whole and partial notches zoomed" \
    "$chrome: a mouse wheel still zooms"
  ui_expect_log 'TestDriver: done' "$chrome: the steps completed"
done
ui_finish 'UI navigation device scroll'
