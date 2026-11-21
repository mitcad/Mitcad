#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/annotations.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-standard.mitcad-ui tools/ui-scripts/drawing-standard.json
# A drawing's dimensioning standard (mitcad#2047): the shared input script
# tools/ui-scripts/drawing-standard.mitcad-ui of
# tools/ui-drawing-standard-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-standard-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-standard.json" --save "$MITCAD_TEST_WORK/standard.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/drawing-standard.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-standard --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/standard.mitcad"
ui_expect_log '^TestDriver: done' 'Standard ASME relaid the dimension out and undid in one step; Create Drawing kept ANSI'"'"'s ASME'
ui_expect_no_log 'TestDriver: FAIL' 'no failed drawing standard assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing dimensioning standard'
