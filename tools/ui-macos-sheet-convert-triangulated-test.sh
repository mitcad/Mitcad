#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# check-all sources: tools/ui-scripts/sheet-convert-triangulated.mitcad-ui
# check-all sources: tools/ui-scripts/sheet-convert-triangulated.json
# Convert to Sheet Metal by triangulation (mitcad#2442): the shared input
# script tools/ui-scripts/sheet-convert-triangulated.mitcad-ui of
# tools/ui-sheet-convert-triangulated-test.sh in the Qt frontend: a
# shelled rectangle-to-round transition's four outer faces picked with
# Development Approximate, OK, the body kept and its Flat Pattern
# approximate; edited with fewer triangles, its stated deviation grows.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-convert-triangulated-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-convert-triangulated.json" --save "$MITCAD_TEST_WORK/transition.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-convert-triangulated.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-convert-triangulated --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/transition.mitcad"
ui_expect_log '^Added Convert to Sheet Metal as F7$' 'the conversion was added'
ui_expect_log '^TestDriver: done' 'a transition laid flat by triangulation through the panel'
ui_expect_no_log 'TestDriver: FAIL' 'no failed conversion assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Convert to Sheet Metal by triangulation'
