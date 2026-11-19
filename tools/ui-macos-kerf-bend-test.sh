#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp app/commands/SheetCommands.cpp core/model/src/features/sheet.rs core/model/src/sheet app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/kerf-bend.mitcad-ui tools/ui-scripts/kerf-bend.json
# Kerf Bend (mitcad#1939): the shared input script tools/ui-scripts/kerf-bend.mitcad-ui of
# tools/ui-kerf-bend-test.sh in the Qt frontend. A kerf bend edited from the timeline opens in Kerf Bend with its slit lattice, its row pitch changed.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-kerf-bend-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/kerf-bend.json" --save "$MITCAD_TEST_WORK/kerf-bend.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/kerf-bend.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app kerf-bend --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/kerf-bend.mitcad"
ui_expect_log 'Editing F5 with Kerf Bend' 'the kerf bend opened in its own panel'
ui_expect_log 'Edited F5' 'the kerf bend was edited'
ui_expect_log '^TestDriver: done' 'Kerf Bend (mitcad#1939): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Kerf Bend (mitcad#1939)'
