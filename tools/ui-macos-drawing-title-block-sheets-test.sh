#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-title-block-sheets.mitcad-ui tools/ui-scripts/drawing-title-block-sheets.json
# Delete Sheet while the title block is edited (mitcad#1841): the shared
# input script tools/ui-scripts/drawing-title-block-sheets.mitcad-ui of
# tools/ui-drawing-title-block-sheets-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-title-block-sheets-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-title-block-sheets.json" --save "$MITCAD_TEST_WORK/sheets.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/drawing-title-block-sheets.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-title-block-sheets --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/sheets.mitcad"
ui_expect_log '^TestDriver: done' 'Done wrote the block onto the sheet edited; the sheet edited deleted ended the editor'
ui_expect_no_log 'TestDriver: FAIL' 'no failed title block sheets assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing title block with sheets deleted'
