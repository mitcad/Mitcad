#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/RibbonFit.cpp
# check-all sources: app/framework/RibbonLabels.cpp app/resources/layout/default.json
# check-all sources: app/drawings/MainWindowDrawings.cpp app/drawings/DrawingWorkspace.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/drawing-ribbon-fit.mitcad-ui tools/ui-scripts/drawing-ribbon-fit.json
# The DRAWING tab's groups at 1280 px (mitcad#2066) through the real UI, the
# shared input script tools/ui-scripts/drawing-ribbon-fit.mitcad-ui (also
# run by tools/ui-windows-drawing-ribbon-fit-test.ps1 and
# tools/ui-macos-drawing-ribbon-fit-test.sh) on a box with one drawing that
# mitcad-cli makes (tools/ui-scripts/drawing-ribbon-fit.json), in the
# Floating and the Docked layout: in a 1280 x 800 window the drawing's
# Dimensions, Text, Symbols and Tables groups stay whole, with the labels
# under the icons and without (each group shows buttons for its main tools,
# the rest are rows of its menu). The pictures of each layout with and
# without labels are kept in the test's folder (MITCAD_UI_LOG_DIR when
# check-all gives one).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-ribbon-fit-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-ribbon-fit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-ribbon-fit.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-ribbon-fit.json" --save "$work/plate.mitcad" || exit 1
pictures=${MITCAD_UI_LOG_DIR:-$root/build}/ui-drawing-ribbon-fit-pictures
mkdir -p "$pictures"
export MITCAD_TEST_WORK=$pictures
ui_start_display

# fit_states: the DRAWING tab's collapsed groups as each state left them,
# one line each, from the drawing's opening on: "opened=none", then
# "labels=..." and "icons=..." after each change of the labels (the last
# "Ribbon collapsed DRAWING: " line before the next change).
fit_states() {
  awk '
    /^Ribbon collapsed DRAWING: / { state = substr($0, 27) }
    /^Drawing opened: D1 / { if (phase == "") phase = "opened" }
    /^Ribbon labels: (labels|icons) / { if (phase != "") { print phase "=" state; phase = $3 } }
    END { if (phase != "") print phase "=" state }' "$UI_LOG"
}

# run_layout <floating|docked>: the shared script in that layout, then its
# states read from the log.
run_layout() {
  local width states line group seen=""
  export MITCAD_TEST_LAYOUT=$1
  ui_start_app --ui=qt --no-mcp "--chrome=$1" --open "$work/plate.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "drawing ribbon fit driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the drawing's ribbon at 1280 px ($1)"
  width=$(grep -E '^Window layout: window [0-9]+x' "$UI_LOG" | tail -1 | sed -E 's/^Window layout: window ([0-9]+)x.*/\1/')
  [ -n "$width" ] || ui_fail "no window size logged ($1)"
  [ "$width" -ge 1280 ] || ui_fail "the window is $width px wide, not 1280 ($1)"
  states=$(fit_states)
  [ -n "$states" ] || ui_fail "no collapsed groups of the DRAWING tab logged ($1)"
  while IFS= read -r line; do
    for group in DIMENSIONS TEXT SYMBOLS TABLES; do
      [[ ", ${line#*=}, " == *", $group, "* ]] && ui_fail "$group collapsed at $width px ($1, ${line%%=*}): ${line#*=}"
    done
    seen="$seen ${line%%=*}"
    echo "ok   $1, ${line%%=*}: collapsed ${line#*=}"
  done <<< "$states"
  [[ $seen == *" labels"* && $seen == *" icons"* ]] || ui_fail "not both with and without labels ($1):$seen"
  for picture in labels icons; do
    [ -s "$pictures/drawing-ribbon-fit-$1-$picture.png" ] || ui_fail "no picture drawing-ribbon-fit-$1-$picture.png"
  done
}

run_layout floating
run_layout docked
ui_finish 'UI drawing ribbon at 1280 px: Dimensions, Text, Symbols and Tables stay whole'
