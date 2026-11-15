#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverNavigation.cpp tools/ui-scripts/navigation-drag-zoom.mitcad-ui
# A drag zoom follows the pointer's vertical motion (mitcad#1867, GitHub
# mitcad/Mitcad#330) in the macOS VM, as tools/ui-navigation-drag-zoom-test.sh
# does on Linux: the shared steps tools/ui-scripts/navigation-drag-zoom.mitcad-ui
# on the demo block, in the Floating and the Docked layout (application
# events; the trackpad's own zoom is unchanged and checked by
# tools/ui-macos-navigation-test.sh).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-navigation-drag-zoom-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=120
for chrome in floating docked; do
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$chrome
  mkdir -p "$MITCAD_SETTINGS_DIR"
  MITCAD_TEST_INPUT=$here/ui-scripts/navigation-drag-zoom.mitcad-ui MITCAD_TEST_STEP_MS=100 \
    ui_run_app "drag-zoom-$chrome" --ui=qt --chrome="$chrome" --no-recovery --no-mcp --demo
  ui_expect_log 'Drag zoom: straight up came closer, straight down went away, sideways kept the zoom, an upward drag with a sideways wobble came closer at every step \(Shift \+ middle\)' \
    "$chrome: Shift + middle zooms by the vertical motion"
  ui_expect_log '^TestDriver: done' "$chrome: drag zoom completed"
  ui_expect_no_log 'TestDriver: FAIL' "$chrome: no failed drag zoom assertion"
  [ "$UI_FAILURES" = 0 ] || exit 1
done
echo 'PASS: navigation drag zoom'
