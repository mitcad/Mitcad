#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp app/framework/TestDriverSketch.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_spacing.rs core/model-api/src/drawings.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-hole-callout.mitcad-ui tools/ui-scripts/drawing-hole-callout.json
# A drawing's own hole callout (mitcad#2054): the shared input script
# tools/ui-scripts/drawing-hole-callout.mitcad-ui of
# tools/ui-drawing-hole-callout-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-hole-callout-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-hole-callout.json" --save "$MITCAD_TEST_WORK/holed.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/drawing-hole-callout.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-hole-callout --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/holed.mitcad"
ui_expect_log '^Drawing command: .*"type":"hole_callout"' 'the hole callout asked for'
ui_expect_log '^TestDriver: done' 'the callout written, its own text after it, the hole followed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed hole callout assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing hole callout'
