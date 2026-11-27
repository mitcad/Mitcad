#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchItemList.cpp app/sketch/SketchItemList.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchReveal.cpp
# check-all sources: app/sketch/SketchModel.cpp app/sketch/DrawTools.cpp app/MainWindow.cpp
# check-all sources: app/commands/EqualSpacingCommands.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli core/sketch core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-item-list.mitcad-ui tools/ui-scripts/sketch-item-list.json
# Sketch mode's list of the sketch's work steps in the timeline's place
# (mitcad#1750, mitcad#2089) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-item-list.mitcad-ui on the design mitcad-cli makes
# (tools/ui-scripts/sketch-item-list.json), in the Docked and the Floating
# layout: the steps as cells, their hover list, the error badge, Delete
# from a cell, a pattern's step opening Edit Pattern on a double-click and
# Edit Pattern and Free Pattern on a right-click, an Equal Spacing step
# opening Edit Equal Spacing on a double-click and offering it on a
# right-click (mitcad#2099), and a sketch saved
# without steps. As tools/ui-sketch-item-list-test.sh (Linux) and
# tools/ui-windows-sketch-item-list-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-item-list-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-item-list}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-item-list.json" --save "$MITCAD_TEST_WORK/items.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=300
for chrome in docked floating; do
  MITCAD_TEST_INPUT=$here/ui-scripts/sketch-item-list.mitcad-ui \
    ui_run_app "sketch-item-list-$chrome" --ui=qt --no-recovery --no-mcp --chrome=$chrome \
    --open "$MITCAD_TEST_WORK/items.mitcad"
  ui_expect_log "Sketch item list in the timeline's place" "$chrome: the step list in the timeline's place"
  ui_expect_log ' add_dimension x1' "$chrome: the dimension as one step"
  ui_expect_log ' \[error\]' "$chrome: the conflict badge on its step"
  ui_expect_log 'double-clicked: pattern$' "$chrome: a pattern's step opens Edit Pattern"
  ui_expect_log 'menu: Edit Pattern [|] Free Pattern$' "$chrome: a pattern's step's menu"
  ui_expect_log '^Freed pattern k' "$chrome: Free Pattern from the step's menu"
  ui_expect_log 'double-clicked: equal spacing$' "$chrome: an Equal Spacing step opens Edit Equal Spacing"
  ui_expect_log '^Edited equal spacing k' "$chrome: the spacing edited from its step"
  ui_expect_log 'menu: Edit Equal Spacing$' "$chrome: an Equal Spacing step's menu"
  ui_expect_log 'double-clicked: nothing$' "$chrome: the original's step opens nothing"
  ui_expect_log 'Timeline back in its place' "$chrome: the timeline back after the sketch"
  ui_expect_log 'TestDriver: done' "$chrome: the steps listed, hovered, selected, deleted and edited"
  ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' "$chrome: all assertions passed"
done
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch step list; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch step list; evidence in $UI_OUT"
