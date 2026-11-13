#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchItemList.cpp app/sketch/SketchItemList.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchReveal.cpp
# check-all sources: app/sketch/SketchModel.cpp app/sketch/DrawTools.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/order.rs core/sketch/src/steps.rs core/model-api/src/sketch.rs
# check-all sources: core/model-api/src/dispatch.rs core/model/src/document.rs
# check-all sources: tools/ui-scripts/sketch-item-list.mitcad-ui tools/ui-scripts/sketch-item-list.json
# Sketch mode's list of the sketch's work steps in the timeline's place
# (mitcad#1750, after mitcad#1492's list of items), through the real UI on
# a design that mitcad-cli makes (tools/ui-scripts/sketch-item-list.json: a
# box, an empty Sketch2 on its top face, and Sketch3 saved without steps):
# one Project of the face's outline, one Rectangle and one dimension show
# three cells, icons only; the Project cell's hover list lists its items, a
# row highlights its edge and a click selects it; a double-click on the
# dimension's cell opens its value editor; a refused Parallel puts the
# error badge on the Rectangle cell; a click on the Rectangle cell selects
# its items and Delete removes them and the cell; Sketch3 shows one cell
# per item; Finish Sketch brings the timeline back. The steps are
# tools/ui-scripts/sketch-item-list.mitcad-ui.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-item-list-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-item-list.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-item-list.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-item-list.json" --save "$work/items.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/items.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch step list driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "Sketch item list in the timeline's place" "the step list in the timeline's place in sketch mode"
ui_expect_log ' project x' 'the face projected as one Project step'
ui_expect_log ' rectangle x' 'the rectangle as one step'
ui_expect_log ' add_dimension x1' 'the dimension as one step'
ui_expect_log ' hover list: Project: ' "the Project step's hover list"
ui_expect_log ' clicked' 'an item selected from its row'
ui_expect_log ' [error]' 'the conflict badge on its step'
ui_expect_log 'Sketch steps: p1 point, p2 point, c3 line' 'a sketch without steps: one cell per item'
ui_expect_log 'Timeline back in its place' 'the timeline back after the sketch'
ui_expect_log 'TestDriver: done' 'the steps listed, hovered, selected, deleted and edited'
ui_finish 'UI sketch step list'
