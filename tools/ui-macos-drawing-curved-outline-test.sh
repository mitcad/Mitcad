#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawings.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-curved-outline.mitcad-ui tools/ui-scripts/drawing-curved-outline.json
# Dimensions to a curved outline's top and a section's label dragged
# (mitcad#1909): the shared input script
# tools/ui-scripts/drawing-curved-outline.mitcad-ui of
# tools/ui-drawing-curved-outline-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-curved-outline-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-curved-outline.json" --save "$MITCAD_TEST_WORK/bend.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/drawing-curved-outline.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-curved-outline --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/bend.mitcad"
ui_expect_log '^Drawing picked point [(]2[)]: ' 'the top of the outer outline picked as the second pick'
ui_expect_log '^TestDriver: done' 'the overall height read 70, the label moved and put back'
ui_expect_no_log 'TestDriver: FAIL' 'no failed curved outline assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing dimension to a curved outline and a dragged section label'
