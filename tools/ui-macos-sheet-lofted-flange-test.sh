#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# check-all sources: tools/ui-scripts/sheet-lofted-flange.mitcad-ui tools/ui-scripts/sheet-lofted-flange.json
# Lofted Flange (mitcad#2512): the shared input script
# tools/ui-scripts/sheet-lofted-flange.mitcad-ui of
# tools/ui-sheet-lofted-flange-test.sh in the Qt frontend: a rectangle and a
# circle 70 mm above it picked as the profiles, OK, the press brake
# transition one body; the lofted flange edited loads its profiles.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-lofted-flange-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-lofted-flange.json" --save "$MITCAD_TEST_WORK/transition.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-lofted-flange.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-lofted-flange --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/transition.mitcad"
ui_expect_log '^Added Lofted Flange as F5$' 'the lofted flange was added'
ui_expect_log '^TestDriver: done' 'a press brake transition made through the panel'
ui_expect_no_log 'TestDriver: FAIL' 'no failed lofted flange assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Lofted Flange'
