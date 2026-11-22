#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/resources/layout/default.json
# check-all sources: core/model/src/features/sheet_perforate.rs core/model/src/sheet/folded/perforate.rs
# check-all sources: tools/ui-scripts/sheet-perforate.mitcad-ui tools/ui-scripts/sheet-perforate.json
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-perforate-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=180
design=$UI_OUT/perforate.mitcad
"$2" run "$here/ui-scripts/sheet-perforate.json" --save "$design" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-perforate.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-perforate --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log '^Preview Perforate: ok.*The perforation cuts 11 holes[.]' 'the outline limits the field'
ui_expect_log '^TestDriver: done' 'all Perforate steps completed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Perforate'
