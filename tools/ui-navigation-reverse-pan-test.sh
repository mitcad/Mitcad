#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/Navigation.hpp app/view/NavigationPreferences.hpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriver.cpp app/framework/TestDriverNavigation.cpp
# check-all sources: tools/ui-scripts/navigation-reverse-pan.mitcad-ui
# check-all sources: tools/ui-scripts/navigation-reverse-pan-restored.mitcad-ui
# Reverse vertical pan (mitcad#1873, GitHub mitcad/Mitcad#340) through the
# real UI, the shared input scripts
# tools/ui-scripts/navigation-reverse-pan.mitcad-ui and
# navigation-reverse-pan-restored.mitcad-ui (also run by
# tools/ui-windows-navigation-reverse-pan-test.ps1 and
# tools/ui-macos-navigation-test.sh) on the demo block: the three
# Reverse flags of Preferences > Navigation are off by default; with
# Reverse vertical pan on, a vertical two-finger scroll pans the view the
# other way and a horizontal one the same way; the setting is saved, read
# back by a new process (the Floating layout, then the Docked one) and
# turned off there again.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-navigation-reverse-pan-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
# The Preferences dialog keeps its changes with OK, its default button.
export MITCAD_TEST_PREFERENCES_DONE=Return
ui_start_display
for run in reverse-pan:floating reverse-pan-restored:docked; do
  script=${run%%:*}
  chrome=${run##*:}
  echo "=== $script ($chrome)"
  export MITCAD_TEST_INPUT=$here/ui-scripts/navigation-$script.mitcad-ui
  ui_start_app --ui=qt --no-mcp --demo --chrome="$chrome"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$script: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$script: the driver failed"; exit 1; }
  ui_expect_log 'TestDriver: the view panned the opposite way' \
    "$script: a vertical two-finger scroll pans the other way when Reverse vertical pan changes"
  if [ "$script" = reverse-pan ]; then
    ui_expect_log 'TestDriver: the view panned the same way' \
      "$script: Reverse vertical pan leaves the horizontal scroll as it was"
  fi
  ui_expect_log 'TestDriver: done' "$script: the steps completed"
done
ui_finish 'UI navigation reverse pan'
