#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/resources/layout/default.json
# check-all sources: core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# check-all sources: tools/ui-scripts/sheet-hem.mitcad-ui tools/ui-scripts/sheet-hem.json
# The Hem command (mitcad#2112): the shared input script
# tools/ui-scripts/sheet-hem.mitcad-ui of tools/ui-sheet-hem-test.sh in the
# Qt frontend: a Flat hem made, changed to Open, Rolled and Double, unfolded
# and refolded.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-hem-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-hem.json" --save "$MITCAD_TEST_WORK/hem.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-hem.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-hem --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/hem.mitcad"
ui_expect_log '^Editing F4 with Hem' 'the hem opened in Hem'
ui_expect_log '^TestDriver: done' 'a hem made, changed to Open, Rolled and Double, unfolded and refolded'
ui_expect_no_log 'TestDriver: FAIL' 'no failed hem assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Hem command'
