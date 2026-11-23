#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_flat.rs
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-flat-pattern-view.mitcad-ui tools/ui-scripts/drawing-flat-pattern-view.json
# A base view of a sheet-metal body's Flat Pattern (mitcad#2280): the shared
# input script tools/ui-scripts/drawing-flat-pattern-view.mitcad-ui of
# tools/ui-drawing-flat-pattern-view-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-flat-pattern-view-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-flat-pattern-view.json" --save "$MITCAD_TEST_WORK/plate.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/drawing-flat-pattern-view.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-flat-pattern-view --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plate.mitcad"
ui_expect_log '"type":"flat_pattern"' 'the Flat Pattern view placed from the Shows list'
ui_expect_log '^Drawing annotations drawn: 1 = 20' 'the bend line dimensioned'
ui_expect_log '^TestDriver: done' 'placed, dimensioned, its bend notes turned off'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Flat Pattern view assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing Flat Pattern view'
