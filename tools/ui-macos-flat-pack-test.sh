#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model/src/features/flat_pack.rs core/model/src/features/finger_joint.rs core/model-api/src/sheet_process.rs core/model-api/src/cut_sheets.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/flat-pack.mitcad-ui tools/ui-scripts/flat-pack.json
# Flat Pack (mitcad#1939): the shared input script tools/ui-scripts/flat-pack.mitcad-ui of
# tools/ui-flat-pack-test.sh in the Qt frontend. A box picked in the browser becomes six finger-jointed plates on a cut sheet, taken back by one undo.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-flat-pack-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/flat-pack.json" --save "$MITCAD_TEST_WORK/flat-pack.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/flat-pack.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app flat-pack --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/flat-pack.mitcad"
ui_expect_log 'Flat pack: six plates of 6' 'the flat pack was made'
ui_expect_log '^TestDriver: done' 'Flat Pack (mitcad#1939): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Flat Pack (mitcad#1939)'
