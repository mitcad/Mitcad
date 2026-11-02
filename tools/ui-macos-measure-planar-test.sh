#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp core/model-api/src/analysis.rs tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-measure-planar-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/cli/tests/measure_planar.json" --save "$MITCAD_TEST_WORK/stack.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/measure-planar.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app measure-planar --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/stack.mitcad"
ui_expect_log '^TestDriver: done' 'plane gap, separate minimum and perpendicular dimension'
ui_expect_no_log 'TestDriver: FAIL' 'no failed planar Measure assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt planar Measure'
