#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/AutoConstraint.cpp app/sketch/SketchOverlay.cpp
# check-all sources: app/sketch/SketchController.cpp app/framework/TestDriver.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui tools/ui-scripts/sketch-horizontal-vertical-other.json
# Horizontal/Vertical takes the other direction when only that can be
# added (mitcad#2184) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/sketch-horizontal-vertical-other.json):
# two points nearer to level whose heights are held; Shift+V, the first
# point, the pointer on the second shows the plumb glyph, and the click
# adds a vertical alignment instead of refusing; one undo step takes it
# away. As tools/ui-sketch-horizontal-vertical-other-test.sh (Linux) and
# tools/ui-windows-sketch-horizontal-vertical-other-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-horizontal-vertical-other-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-horizontal-vertical-other}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-horizontal-vertical-other.json" \
  --save "$MITCAD_TEST_WORK/horizontal-vertical-other.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui \
  ui_run_app sketch-horizontal-vertical-other --ui=qt --no-recovery --no-mcp \
  --open "$MITCAD_TEST_WORK/horizontal-vertical-other.mitcad"
ui_expect_log 'Horizontal/Vertical would add vertical_points on p1, p2' 'the hover glyph shows plumb'
ui_expect_log 'Added constraint vertical_points k[0-9]+ on p1, p2' 'Horizontal/Vertical added plumb instead of refusing'
ui_expect_log 'TestDriver: done' 'p2 slid under p1, one undo step'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Horizontal/Vertical in the other direction; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Horizontal/Vertical in the other direction; evidence in $UI_OUT"
