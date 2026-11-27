#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/SketchCurveRows.cpp app/MainWindowBrowserCurves.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowNoticeHover.cpp app/commands/SurfaceCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/surface-patch-browser-curve.mitcad-ui
# Patch takes a sketch curve for its boundary from the Browser (mitcad#1978,
# follow-up of mitcad#1972, GitHub mitcad/Mitcad#407), through the real UI:
# the shared input script tools/ui-scripts/surface-patch-browser-curve.mitcad-ui
# (also run by tools/ui-windows-surface-patch-browser-curve-test.ps1 and
# tools/ui-macos-surface-patch-browser-curve-test.sh) on the reported design
# of the private test files (MITCAD_MITCAD_REPORTED,
# github-407/surfacing_bug.zip; never committed), unpacked into the test's
# own folder: while Patch's boundary is active the Browser lists the
# sketches' curves; with Whole Opening off Patch takes the wall's top and
# slanted free edges in the view and the sketch line from its Browser row
# (its hover paints it), and OK adds the patch of the triangle they close.
# Skipped without the design.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-surface-patch-browser-curve-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
zip=${MITCAD_MITCAD_REPORTED:-}/github-407/surfacing_bug.zip
if [ -z "${MITCAD_MITCAD_REPORTED:-}" ] || [ ! -f "$zip" ] || ! command -v unzip > /dev/null; then
  echo "skip: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-407) or unzip is not here"
  exit 0
fi
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-surface-patch-browser-curve.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/surface-patch-browser-curve.mitcad-ui
# The design in the test's folder: the run writes nothing next to the
# private test files.
unzip -o -q "$zip" -d "$work/reported" || { echo "FAIL: the reported design could not be unpacked" >&2; exit 1; }
design=$(find "$work/reported" -name '*.mitcad' -type f | sort | head -1)
[ -n "$design" ] || { echo "FAIL: no design in the reported zip" >&2; exit 1; }
ui_start_display
UI_WINDOW_WAIT=180 ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'surface patch browser curve driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log ' sketches, ' 'the Browser lists the sketches'"'"' curves during Patch'
ui_expect_log 'Notice hover highlighted: browser curve: sketch_curve c42 ' 'the curve'"'"'s row paints it under the pointer'
ui_expect_log 'Patch Profiles or Edges: 2 edges, 1 sketch curve' 'two free edges and the line from the Browser are taken'
ui_expect_log 'Added patch of 3 items' 'OK adds the patch'
ui_expect_log 'Browser sketch curves: none' 'the curve rows go with the command'
ui_expect_log 'TestDriver: done' 'every step passed'
ui_finish 'UI patch with a sketch curve picked in the Browser'
