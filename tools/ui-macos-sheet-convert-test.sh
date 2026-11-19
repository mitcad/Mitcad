#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# check-all sources: tools/ui-scripts/sheet-convert.mitcad-ui tools/ui-scripts/sheet-convert.json
# Convert to Sheet Metal (mitcad#1949): the shared input script
# tools/ui-scripts/sheet-convert.mitcad-ui of tools/ui-sheet-convert-test.sh
# in the Qt frontend: a solid box's bottom face and its four bottom edges
# picked, OK, the tray one body, its corners ripped; the conversion edited
# loads its picks.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-convert-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-convert.json" --save "$MITCAD_TEST_WORK/tray.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-convert.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-convert --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/tray.mitcad"
ui_expect_log '^Added Convert to Sheet Metal as F3$' 'the conversion was added'
ui_expect_log '^TestDriver: done' 'a solid tray converted through the panel, its corners ripped'
ui_expect_no_log 'TestDriver: FAIL' 'no failed conversion assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Convert to Sheet Metal'
