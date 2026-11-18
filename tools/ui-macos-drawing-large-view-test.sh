#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser/DocumentHost.hpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/ffi/src/kernel/mod.rs
# check-all sources: tools/ui-scripts/drawing-large-view.mitcad-ui tools/ui-scripts/drawing-large-view.json
# A large design's drawing (mitcad#1997): the shared input script
# tools/ui-scripts/drawing-large-view.mitcad-ui of
# tools/ui-drawing-large-view-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-large-view-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-large-view.json" --save "$MITCAD_TEST_WORK/cylinders.mitcad" || exit 1
export MITCAD_TEST_RECOMPUTE_DELAY_MS='add_drawing_view=3000,edit_drawing_view=3000,drawing sheet=3000'
MITCAD_TEST_INPUT=$here/ui-scripts/drawing-large-view.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-large-view --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/cylinders.mitcad"
ui_expect_log '^Drawing note: Drawing the views' 'the opened sheet drawn in a job, with its note'
ui_expect_log '^Drawing views updating: 1 by 0[.]5$' 'the old lines scaled while the new scale is projected'
ui_expect_log '^Computing add_drawing_view cancelled after ' 'the placing cancelled'
ui_expect_log '^TestDriver: done' 'placed once, cancelled without a view, drawn in jobs after a model change'
ui_expect_no_log 'TestDriver: FAIL' 'no failed large view assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing of a large design: jobs, progress, cancel and the old lines scaled'
