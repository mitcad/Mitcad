#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/RibbonFit.cpp
# check-all sources: app/framework/RibbonLabels.cpp app/resources/layout/default.json
# check-all sources: app/drawings/MainWindowDrawings.cpp app/drawings/DrawingWorkspace.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/drawing-ribbon-fit.mitcad-ui tools/ui-scripts/drawing-ribbon-fit.json
# The DRAWING tab's groups at 1280 px (mitcad#2066): the shared input script
# tools/ui-scripts/drawing-ribbon-fit.mitcad-ui of
# tools/ui-drawing-ribbon-fit-test.sh in the Qt frontend, in the Floating
# and the Docked layout: the Dimensions, Text, Symbols and Tables groups
# stay whole with the labels under the icons and without. A screen
# narrower than 1280 px is said and not checked. The pictures are kept in
# <output>/files.
# Usage: tools/ui-macos-drawing-ribbon-fit-test.sh <app> <cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-ribbon-fit-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-ribbon-fit.json" --save "$MITCAD_TEST_WORK/plate.mitcad" || exit 1

# fit_states: the DRAWING tab's collapsed groups as each state left them,
# as tools/ui-drawing-ribbon-fit-test.sh reads them.
fit_states() {
  awk '
    /^Ribbon collapsed DRAWING: / { state = substr($0, 27) }
    /^Drawing opened: D1 / { if (phase == "") phase = "opened" }
    /^Ribbon labels: (labels|icons) / { if (phase != "") { print phase "=" state; phase = $3 } }
    END { if (phase != "") print phase "=" state }' "$UI_LOG_FILE"
}

for layout in floating docked; do
  MITCAD_TEST_LAYOUT=$layout MITCAD_TEST_INPUT=$here/ui-scripts/drawing-ribbon-fit.mitcad-ui MITCAD_TEST_STEP_MS=100 \
    ui_run_app "drawing-ribbon-fit-$layout" --ui=qt --no-recovery --no-mcp "--chrome=$layout" \
    --open "$MITCAD_TEST_WORK/plate.mitcad"
  ui_expect_log '^TestDriver: done' "the drawing's ribbon at 1280 px ($layout)"
  ui_expect_no_log 'TestDriver: FAIL' "no failed drawing ribbon assertion ($layout)"
  width=$(grep -E '^Window layout: window [0-9]+x' "$UI_LOG_FILE" | tail -1 | sed -E 's/^Window layout: window ([0-9]+)x.*/\1/')
  if [ -z "$width" ]; then
    ui_fail "no window size logged ($layout)"
    continue
  fi
  if [ "$width" -lt 1280 ]; then
    echo "skip the window is $width px wide, narrower than 1280 ($layout)"
    continue
  fi
  states=$(fit_states)
  [ -n "$states" ] || ui_fail "no collapsed groups of the DRAWING tab logged ($layout)"
  seen=""
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    for group in DIMENSIONS TEXT SYMBOLS TABLES; do
      if [[ ", ${line#*=}, " == *", $group, "* ]]; then
        ui_fail "$group collapsed at $width px ($layout, ${line%%=*}): ${line#*=}"
      fi
    done
    seen="$seen ${line%%=*}"
  done <<< "$states"
  if [[ $seen == *" labels"* && $seen == *" icons"* ]]; then
    ui_pass "Dimensions, Text, Symbols and Tables whole at $width px ($layout:$seen)"
  else
    ui_fail "not both with and without labels ($layout):$seen"
  fi
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing ribbon at 1280 px: Dimensions, Text, Symbols and Tables stay whole'
