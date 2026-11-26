#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp core/model/src/features/shell.rs geometry/src/faceops.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/shell-own-thickness.mitcad-ui tools/ui-scripts/shell-own-thickness.json
# Shell with faces of their own thickness (mitcad#2443): the shared input
# script tools/ui-scripts/shell-own-thickness.mitcad-ui of
# tools/ui-shell-own-thickness-test.sh in the Qt frontend. Shell with the top
# removed, 2 mm inside, the front in an Own Thickness row at 5 mm, then
# reopened and the row edited to 4 mm, with the volumes the script works out.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-shell-own-thickness-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/shell-own-thickness.json" --save "$MITCAD_TEST_WORK/shell-own-thickness.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/shell-own-thickness.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app shell-own-thickness --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/shell-own-thickness.mitcad"
ui_expect_log 'Shell: added face_thicknesses row 1' 'the Own Thickness row was added'
ui_expect_log 'Added shell removing 1 face[(]s[)], 1 face[(]s[)] at their own thickness' 'the shell was added with the row'
ui_expect_log '^TestDriver: done' 'Shell with faces of their own thickness (mitcad#2443): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Shell with faces of their own thickness (mitcad#2443)'
