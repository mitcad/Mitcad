#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/RememberedValues.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sheet-flange-remembered.mitcad-ui tools/ui-scripts/sheet-flange-remembered.json
# Flange's remembered choices (mitcad#1868): the shared input script
# tools/ui-scripts/sheet-flange-remembered.mitcad-ui of
# tools/ui-sheet-flange-remembered-test.sh in the Qt frontend, with the
# remembered values on (the library turns them off) in the run's own
# settings.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-flange-remembered-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_REMEMBER_PANEL_VALUES=1
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-flange-remembered.json" --save "$MITCAD_TEST_WORK/flanges.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-flange-remembered.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-flange-remembered --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/flanges.mitcad"
ui_expect_log '^Panel Flange position: remembered outside' 'Flange again started with the last OK'"'"'s Position'
ui_expect_log '^TestDriver: done' 'a new Flange starts with the last OK'"'"'s Measured and Position, an edit loads its own'
ui_expect_no_log 'TestDriver: FAIL' 'no failed remembered choice assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Flange remembered choices'
