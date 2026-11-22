#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/AutoConstraint.cpp app/sketch/SketchOverlay.cpp
# check-all sources: app/sketch/SketchController.cpp app/framework/TestDriver.cpp core/sketch core/solver
# check-all sources: core/model-api/src/sketch.rs core/model/src/document.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui tools/ui-scripts/sketch-horizontal-vertical-other.json
# Horizontal/Vertical takes the other direction when only that can be
# added (mitcad#2184) through the real UI on a design that mitcad-cli
# makes (tools/ui-scripts/sketch-horizontal-vertical-other.json): two
# points nearer to level whose heights are held; Shift+V, the first point,
# the pointer on the second shows the plumb glyph, and the click adds a
# vertical alignment instead of refusing; one undo step takes it away.
# The steps are tools/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui,
# which tools/ui-windows-sketch-horizontal-vertical-other-test.ps1 runs on
# Windows and tools/ui-macos-sketch-horizontal-vertical-other-test.sh on
# macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-horizontal-vertical-other-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-horizontal-vertical-other.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=${MITCAD_UI_LOG_DIR:-$work} MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-horizontal-vertical-other.json" --save "$work/horizontal-vertical-other.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/horizontal-vertical-other.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Horizontal/Vertical other direction driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Horizontal/Vertical would add vertical_points on p1, p2' 'the hover glyph shows plumb'
ui_expect_log 'Added constraint vertical_points k' 'Horizontal/Vertical added plumb instead of refusing'
ui_expect_log 'TestDriver: done' 'p2 slid under p1, one undo step'
ui_finish 'UI sketch Horizontal/Vertical in the other direction'
