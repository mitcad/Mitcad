#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model-api/src/sheet_process.rs core/model/src/sheet app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/board-box.mitcad-ui tools/ui-scripts/board-box.json
# Standard boxes of board (mitcad#1939): the shared input script tools/ui-scripts/board-box.mitcad-ui of
# tools/ui-board-box-test.sh in the Qt frontend. A FEFCO 0201 box of corrugated board made by Standard Box, folded with its glue flap touching the first panel, Finger Joint greyed for the board, the box taken back by one undo.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-board-box-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/board-box.json" --save "$MITCAD_TEST_WORK/board-box.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/board-box.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app board-box --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/board-box.mitcad"
ui_expect_log 'Standard box fefco_0201, ' 'the standard box was made'
ui_expect_log '^TestDriver: done' 'Standard boxes of board (mitcad#1939): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Standard boxes of board (mitcad#1939)'
