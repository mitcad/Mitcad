#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/SelectionScope.cpp app/framework/SelectionScope.hpp app/framework/SelectionFilterPanel.cpp
# check-all sources: app/MainWindowSelection.cpp app/sketch app/framework/TestDriver.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-filter-annotations.mitcad-ui tools/ui-scripts/sketch-filter-annotations.json
# The sketch Selection Filter's Dimensions and Constraints rows (GitHub
# #388) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-filter-annotations.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/sketch-filter-annotations.json). With
# only Dimensions a cross a window takes only the dimension; with only
# Constraints a cross only the constraints, the Fix symbol among them; with
# Dimensions empty a click on the dimension does not select it, one on the
# Parallel does. As tools/ui-sketch-filter-annotations-test.sh (Linux) and
# tools/ui-windows-sketch-filter-annotations-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-filter-annotations-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-filter-annotations}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-filter-annotations.json" --save "$MITCAD_TEST_WORK/annotations.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-filter-annotations.mitcad-ui \
  ui_run_app sketch-filter-annotations --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/annotations.mitcad"
ui_expect_log 'Selection filter sketch dimensions cross: ' 'only Dimensions a cross'
ui_expect_log 'Selection filter sketch constraints cross: ' 'only Constraints a cross'
ui_expect_log 'Selection filter sketch dimensions empty: ' 'Dimensions empty'
ui_expect_log 'TestDriver: done' 'a window and a click each followed their own row'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch filter Dimensions and Constraints rows; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch filter Dimensions and Constraints rows; evidence in $UI_OUT"
