#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchItemList.cpp app/sketch/SketchItemList.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchReveal.cpp
# check-all sources: app/sketch/SketchModel.cpp app/sketch/DrawTools.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/order.rs core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-item-list.mitcad-ui tools/ui-scripts/sketch-item-list.json
# Sketch mode's list of the sketch's items in the timeline's place
# (mitcad#1492), through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-item-list.json: an empty Sketch1): editing it
# puts the list in the timeline's place; a rectangle drawn with its width
# and height typed lists its lines, corners and constraints, then its two
# dimensions, in creation order; a line selected from its row; the view's
# selection marks a row; a constraint deleted from its row (the sketch one
# degree of freedom freer); the width edited from its row; Finish Sketch
# brings the timeline back. The steps are
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
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch item list driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "Sketch item list in the timeline's place" "the list in the timeline's place in sketch mode"
ui_expect_log 'Sketch items: c1 line, ' 'the rectangle listed in creation order'
ui_expect_log 'Sketch item rows selected: c1' 'a line selected from its row'
ui_expect_log 'Deleted k' 'a constraint deleted from its row'
ui_expect_log 'Timeline back in its place' 'the timeline back after the sketch'
ui_expect_log 'TestDriver: done' 'the rows in creation order, selected, deleted and edited'
ui_finish 'UI sketch item list'
