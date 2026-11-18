#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/framework/ResultRows.cpp app/framework/CommandSession.cpp app/framework/NoticePlaces.cpp app/MainWindowNoticeHover.cpp app/OcctViewer.cpp core/model-api/src/analysis.rs tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-measure-guides-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/cli/tests/measure_guides.json" --save "$MITCAD_TEST_WORK/plate.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/measure-guides.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app measure-guides --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plate.mitcad"
ui_expect_log '^TestDriver: done' 'the minimum and the axis distance drawn apart, each row painting its segment'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Measure guides assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Measure guides'
