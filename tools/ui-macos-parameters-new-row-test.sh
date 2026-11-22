#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/platform/MacParameters.hpp app/framework/ParameterUnits.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-new-row.mitcad-ui tools/ui-scripts/parameters-new-row.json
# The Parameters window's empty row, its rows' delete buttons and its
# bottom row (mitcad#2202): the shared input script
# tools/ui-scripts/parameters-new-row.mitcad-ui of
# tools/ui-parameters-new-row-test.sh in the Qt frontend, on the design that
# mitcad-cli makes (tools/ui-scripts/parameters-new-row.json): parameters
# typed one after the other in the empty row, a taken name refused, an
# unused parameter deleted by its row's button and a used one's button
# disabled, the empty row last under a sort and hidden under the search, a
# slider's value shown during the drag, and the bottom row's pictures on
# both tabs (in the output folder).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-parameters-new-row-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_PARAMETERS_SHOTS=$UI_OUT
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/parameters-new-row.json" --save "$MITCAD_TEST_WORK/parameters-new-row.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/parameters-new-row.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app parameters-new-row --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/parameters-new-row.mitcad"
ui_expect_log '^Created user parameter a3 = 13 mm \(mm\) in the new row$' 'three parameters typed one after the other'
ui_expect_log '^New user parameter spare refused: ' 'a taken name is refused and kept'
ui_expect_log '^Deleted user parameter spare by its row button$' "an unused parameter deleted by its row's button"
ui_expect_log '^Parameter slide slider held at ' "the slider's value shown before the release"
ui_expect_log '^TestDriver: done' 'the empty row, the delete buttons and the bottom row'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
for tab in parameters tables; do
  [ -s "$UI_OUT/parameters-bottom-row-$tab.png" ] || ui_fail "no picture of the bottom row on the $tab tab"
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt parameters new row'
