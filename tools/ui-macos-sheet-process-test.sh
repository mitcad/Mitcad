#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/commands/SheetProcessCommands.cpp core/model/src/sheet/process.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sheet-process.mitcad-ui tools/ui-scripts/sheet-process.json
# Sheet process profiles (mitcad#1939): the shared input script tools/ui-scripts/sheet-process.mitcad-ui of
# tools/ui-sheet-process-test.sh in the Qt frontend. Board rules with a crease table made in the Sheet Rules panel, Corner Relief greyed for them, a board sheet, the rules edited loading their profile.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-process-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-process.json" --save "$MITCAD_TEST_WORK/sheet-process.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-process.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-process --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/sheet-process.mitcad"
ui_expect_log 'Sheet rules: thickness .*, board, crease_table' 'board rules with a crease table were added'
ui_expect_log 'Editing F2 with Sheet Rules' 'the rules were edited'
ui_expect_log '^TestDriver: done' 'Sheet process profiles (mitcad#1939): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Sheet process profiles (mitcad#1939)'
