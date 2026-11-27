#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMake.cpp app/drawings/NestFlatPatternsDialog.cpp app/drawings/CutSheetCutting.cpp app/drawings/DrawingSheetView.cpp app/drawings/MainWindowDrawings.cpp core/model-api/src/cut_sheets.rs core/model-api/src/cut_sheets/cutting.rs core/model-drawing/src/cut_sheet.rs core/cam/src/sheet_cut.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/nest-flat-patterns.mitcad-ui tools/ui-scripts/nest-flat-patterns.json
# Nest Flat Patterns and the plasma cutting program (mitcad#2553,
# mitcad#2570) through the real dialogs: the input script
# tools/ui-scripts/nest-flat-patterns.mitcad-ui on a design that mitcad-cli
# makes (tools/ui-scripts/nest-flat-patterns.json). Two sheet-metal
# components with copies are nested onto a new cut sheet; a part dragged
# onto another is refused in words and the program is not offered; dragged
# back, the cutting paths are drawn and the program is written. The
# program is then read here: the round hole cut as G3 arcs (its two
# halves, the part on the right), no clockwise arc (the outlines are
# rectangles), the torch fired once per loop.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-nest-flat-patterns-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-nest-flat-patterns.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/nest-flat-patterns.mitcad-ui
design=$work/nest-flat-patterns.mitcad
"$cli" run "$here/ui-scripts/nest-flat-patterns.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'nest-flat-patterns driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Nest Flat Patterns: 3 part(s) on CutSheet1' 'the Flat Patterns were nested'
ui_expect_log 'Cutting program check: Two components overlap' 'the overlap was said in words'
ui_expect_log 'TestDriver: done' 'Nest Flat Patterns (mitcad#2570): every step of the script'
program=$work/sheet.nc
[ -s "$program" ] || ui_fail "no cutting program written to $program"
arcs=$(grep -c '^G3 ' "$program")
clockwise=$(grep -c '^G2 ' "$program")
fired=$(grep -c '^M3 S1000$' "$program")
if [ "$arcs" -lt 2 ] || [ "$clockwise" -ne 0 ] || [ "$fired" -ne 4 ]; then
  cat "$program" >&2
  ui_fail "the program has $arcs G3 and $clockwise G2 lines and fires the torch $fired times; wanted the hole's G3 arcs, no G2 and 4 firings"
fi
centred=$(grep '^G3 ' "$program" | grep -c ' I.* J')
[ "$centred" -eq "$arcs" ] || ui_fail 'a G3 line without its centre (I, J)'
ui_finish 'UI nest-flat-patterns'
