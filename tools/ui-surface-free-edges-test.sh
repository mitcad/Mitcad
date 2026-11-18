#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/MainWindow.cpp app/MainWindowSelection.cpp app/framework/Selection.cpp
# check-all sources: app/framework/OpeningEdges.cpp app/framework/Command.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandSessionHover.cpp app/framework/TestDriver.cpp app/commands/SurfaceCommands.cpp
# check-all sources: core/model/src/features/extend_surface.rs geometry/src/surface_offsets.cpp
# check-all sources: tools/ui-scripts/surface-free-edges.mitcad-ui
# A surface body's free edges with no command, and Extend's Whole Opening
# (mitcad#1955, GitHub mitcad/Mitcad#407), through the real UI: the shared
# input script tools/ui-scripts/surface-free-edges.mitcad-ui (also run by
# tools/ui-windows-surface-free-edges-test.ps1 and
# tools/ui-macos-surface-free-edges-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, github-407/surfacing_bug.zip;
# never committed), unpacked into the test's own folder: with no command a
# free edge hovers and selects as an edge, Extend started then takes it as
# its whole opening, and with Whole Opening off the single edge extends.
# Skipped without the design.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-surface-free-edges-test.sh
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
work=$(mktemp -d "$root/build/ui-surface-free-edges.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/surface-free-edges.mitcad-ui
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
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'surface free edges driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Idle hover: edge {"at":' 'with no command a free edge hovers as an edge'
ui_expect_log 'Selected: 1 edge [edge {"at":' 'a click selects the free edge'
ui_expect_log 'Extend Edges: 1 edge [edge {"faces":' 'Extend takes it as its whole opening'
ui_expect_log 'Hover opening: 4 edge' 'the hover shows the whole opening'
ui_expect_log 'Added extend of 1 edges' 'with Whole Opening off the single edge extends'
ui_expect_log 'TestDriver: done' 'every step passed'
ui_finish 'UI surface free edges with no command and Whole Opening'
