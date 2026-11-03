#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/commands/CommandSupport.cpp
# check-all sources: app/commands/ProfilePicks.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/extrude-from-sketch.mitcad-ui
# Extrude (E) started from a zoomed sketch on a box's top face (mitcad#1227,
# mitcad#1226) through the real UI, the shared input script
# tools/ui-scripts/extrude-from-sketch.mitcad-ui (also run by
# tools/ui-windows-extrude-from-sketch-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/extrude-from-sketch.json): the sketch and the box
# come back inside the view's free area, not under the ribbon, the cards
# or the cube, and the automatic profile is a drawn circle, not the face's
# leftover region.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-from-sketch-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-extrude-from-sketch.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/extrude-from-sketch.mitcad-ui
"$cli" run "$here/ui-scripts/extrude-from-sketch.json" --save "$work/extrude-from-sketch.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/extrude-from-sketch.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Extrude from a sketch driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Extrude from a zoomed sketch shows the sketch and its body, a drawn profile taken'
ui_finish 'UI Extrude from a zoomed sketch'
