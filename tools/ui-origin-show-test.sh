#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowVisibility.cpp app/MainWindow.cpp
# check-all sources: app/OcctViewer.cpp app/framework/TestDriver.cpp app/framework/ModelShapes.cpp
# check-all sources: core/model/src/document/display.rs tools/ui-scripts/origin-show.mitcad-ui
# The design's origin shown and hidden both ways (mitcad#1642, GitHub
# mitcad/Mitcad#240) through the real UI, the shared input script
# tools/ui-scripts/origin-show.mitcad-ui (also run by
# tools/ui-windows-origin-show-test.ps1) on a new design: the Origin
# folder's eye and View › Show › Origins each show the origin's planes,
# axes and point (the other control turned on in the same undo step) and
# hide them again, read from the view itself ("View origin drawn: ...",
# expect-view-origin: the planes' colour in the view's frame too).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-origin-show-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-origin-show.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/origin-show.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'origin show driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "TestDriver: view origin: planes xy, xz, yz; axes x, y, z; point origin; " \
  "the origin's planes, axes and point drawn"
ui_expect_log "TestDriver: view origin: planes none; axes none; point none; " 'the origin hidden again'
ui_expect_log 'Undo: Show Origin' "the eye showed the origin in one undo step"
ui_expect_log 'Undo: Show All Origins' "View › Show › Origins showed it in one undo step"
ui_expect_log 'TestDriver: done' 'the origin shows and hides with the eye and with View › Show › Origins'
ui_finish 'UI origin shown both ways'
