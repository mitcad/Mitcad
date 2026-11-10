#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/Navigation.hpp app/view/ViewNavigation.hpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverNavigation.cpp app/framework/GlassCard.cpp
# check-all sources: tools/ui-scripts/navigation-gesture-state.mitcad-ui
# A press on the 3D view stays what it began as until its button is up
# (mitcad#1571, #1572, #1573) through the real UI, the shared input script
# tools/ui-scripts/navigation-gesture-state.mitcad-ui (also run by
# tools/ui-windows-navigation-gesture-state-test.ps1 and
# tools/ui-macos-navigation-test.sh) on the demo block, in the Floating and
# the Docked layout: keys pressed or released during a press keep its
# gesture and its click, a trackpad scroll while a button is held leaves
# the press a click, a scroll's end lets its button up at once, and the
# window deactivated during a drag ends it, as does F4 released elsewhere.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-navigation-gesture-state-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/navigation-gesture-state.mitcad-ui
ui_start_display
for chrome in floating docked; do
  echo "=== $chrome"
  ui_start_app --ui=qt --no-mcp --demo --chrome="$chrome"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$chrome: navigation gesture state driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$chrome: navigation gesture state driver failed"; exit 1; }
  ui_expect_log 'Navigation gesture state: keys changed during a press kept its gesture and its click' \
    "$chrome: keys changed during a press"
  ui_expect_log 'Navigation gesture state: a scroll with a button held' "$chrome: scrolls during a press"
  ui_expect_log 'Navigation gesture state: a deactivated window ended F4, Option and scroll drags' \
    "$chrome: the window deactivated during a drag"
  ui_expect_log 'TestDriver: done' "$chrome: Ctrl released before a moved click, and F4 released elsewhere"
done
ui_finish 'UI navigation gesture state'
