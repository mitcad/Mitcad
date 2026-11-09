#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchOverlay.cpp
# check-all sources: app/sketch/SketchSnap.cpp app/sketch/DrawTools.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-drag-join.mitcad-ui tools/ui-scripts/sketch-drag-join.json
# A dragged sketch point snaps to points and curves and is held where it is
# dropped, in the drag's undo step (mitcad#1538, GitHub mitcad/Mitcad#198),
# through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-drag-join.json): the Line tool draws an open
# polyline; its last point dragged onto its first with Shift held is not
# held there, and without Shift it snaps to the first point, is held there
# and closes the profile, one undo step with the drag. The steps are
# tools/ui-scripts/sketch-drag-join.mitcad-ui, which
# tools/ui-windows-sketch-drag-join-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-drag-join-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-drag-join.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-drag-join.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-drag-join.json" --save "$work/join.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/join.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'drag join driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'off (Shift or Alt)' 'Shift turns the drag snap off'
ui_expect_log ': held there' 'the point dropped on the first point is held there'
ui_expect_log 'TestDriver: done' 'a dragged point snaps and is held where it is dropped, not with Shift'
ui_finish 'UI sketch drag snaps and holds the dropped point'
