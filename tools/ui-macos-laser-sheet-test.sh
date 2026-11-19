#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model/src/features/finger_joint.rs core/model/src/features/t_nut_slot.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/laser-sheet.mitcad-ui tools/ui-scripts/laser-sheet.json
# Laser sheet joints (mitcad#1939): the shared input script tools/ui-scripts/laser-sheet.mitcad-ui of
# tools/ui-laser-sheet-test.sh in the Qt frontend. A finger joint of two plates at a corner selected in the browser, then a T-nut slot from a plate's end face with the screw's hole in its mate, with the volumes cli.sheet_laser_joints gives.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-laser-sheet-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/laser-sheet.json" --save "$MITCAD_TEST_WORK/laser-sheet.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/laser-sheet.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app laser-sheet --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/laser-sheet.mitcad"
ui_expect_log 'Added finger joint of 2 plates' 'the finger joint was added'
ui_expect_log 'Added T-nut slot with the screw.s hole' 'the T-nut slot was added'
ui_expect_log '^TestDriver: done' 'Laser sheet joints (mitcad#1939): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Laser sheet joints (mitcad#1939)'
