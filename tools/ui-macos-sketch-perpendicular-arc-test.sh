#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchController.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-perpendicular-arc.mitcad-ui tools/ui-scripts/sketch-perpendicular-arc.json
# Perpendicular of a line and an arc (mitcad#2114) in the Qt frontend on
# macOS: the shared steps tools/ui-scripts/sketch-perpendicular-arc.mitcad-ui
# on the design mitcad-cli makes (tools/ui-scripts/sketch-perpendicular-arc.json):
# Shift+R, the line, then the arc add one perpendicular; the line turns
# through the arc's center, its symbol shows, and one undo step takes it
# away. As tools/ui-sketch-perpendicular-arc-test.sh (Linux) and
# tools/ui-windows-sketch-perpendicular-arc-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-perpendicular-arc-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-perpendicular-arc}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-perpendicular-arc.json" --save "$MITCAD_TEST_WORK/perpendicular-arc.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-perpendicular-arc.mitcad-ui \
  ui_run_app sketch-perpendicular-arc --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/perpendicular-arc.mitcad"
ui_expect_log 'Added constraint perpendicular k[0-9]+ on c3, c10' 'Perpendicular took the line and the arc'
ui_expect_log 'glyph at [0-9]+,[0-9]+ [(]perpendicular by c3[)]' "the Perpendicular's symbol shows"
ui_expect_log 'TestDriver: done' 'the line turned through the center, one undo step'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Perpendicular of a line and an arc; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Perpendicular of a line and an arc; evidence in $UI_OUT"
