#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowSheet.cpp app/resources/layout/default.json core/model/src/sheet/bend_table.rs
# check-all sources: core/model/src/document/flat_patterns.rs core/model-api/src/exchange.rs
# check-all sources: tools/ui-scripts/sheet-bend-table.mitcad-ui tools/ui-scripts/sheet-flange-remembered.json
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-bend-table-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK" || exit 1
rm -f -- "$MITCAD_TEST_WORK/bends.csv" "$MITCAD_TEST_WORK/bends.json"
design=$MITCAD_TEST_WORK/bracket.mitcad
"$2" run "$here/ui-scripts/sheet-flange-remembered.json" --save "$design" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-bend-table.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-bend-table --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$design"
ui_expect_log '^TestDriver: done' 'the bend table exports'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
csv=$MITCAD_TEST_WORK/bends.csv
grep -E '^"[^"]+","SheetRules1",2,0[.]3,"F4:0",90,"(up|down)",3,' "$csv" ||
  ui_fail 'the CSV bend row has the expected sheet and bend values'
grep -q '"angle_deg": 90' "$MITCAD_TEST_WORK/bends.json" ||
  ui_fail 'the JSON bend row has the expected angle'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Export Bend Table'
