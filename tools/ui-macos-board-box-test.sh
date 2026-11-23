#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model-api/src/sheet_process.rs core/model-api/src/sheet_process core/model/src/sheet app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/board-box.mitcad-ui tools/ui-scripts/board-box.json
# Standard boxes of board (mitcad#1939): the shared input script tools/ui-scripts/board-box.mitcad-ui of
# tools/ui-board-box-test.sh in the Qt frontend. A FEFCO 0201 box of corrugated board made by Standard Box, folded with its glue flap touching the first panel, Finger Joint greyed for the board, the box taken back by one undo; then a FEFCO 0203 box and a FEFCO 0427 folder chosen from the Style list and built (mitcad#2226); then a FEFCO 0421 folder with its Tuck and Tabs set, kept in its rules, its tuck changed in Edit Standard Box (mitcad#2269), then its number of tabs, the blank made again and the box following, and another style refused by the model in words in the panel's own message area (mitcad#2285).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-board-box-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/board-box.json" --save "$MITCAD_TEST_WORK/board-box.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/board-box.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app board-box --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/board-box.mitcad"
ui_expect_log 'Standard box fefco_0201, ' 'the standard box was made'
ui_expect_log 'Standard box fefco_0203, ' 'a FEFCO 0203 box was made from the Style list (mitcad#2226)'
ui_expect_log 'Standard box fefco_0427, ' 'a FEFCO 0427 folder was made from the Style list (mitcad#2226)'
ui_expect_log ', tuck 25 mm, tabs 3' 'a FEFCO 0421 folder was made with Tuck 25 mm and Tabs 3 (mitcad#2269)'
ui_expect_log ', tuck 30 mm, tabs 3' 'Edit Standard Box changed the folder tuck to 30 mm (mitcad#2269)'
ui_expect_log ', tuck 30 mm, tabs 4' 'Edit Standard Box made the folder blank again with four tabs, the box following (mitcad#2285)'
ui_expect_log 'Panel Standard Box error: another style changes the shape of the box' 'another style was refused in words in the panel (mitcad#2285)'
ui_expect_log 'Standard Box: OK refused by the model: ' 'the model refusal kept the panel open (mitcad#2285)'
ui_expect_log '^TestDriver: done' 'Standard boxes of board (mitcad#1939): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Standard boxes of board (mitcad#1939)'
