#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/commands/PropertiesReport.cpp app/MainWindowNoticeHover.cpp app/framework/NoticePlaces.cpp
# check-all sources: core/model-api/src/analysis.rs core/model-api/src/physical_extent.rs tools/cli
# The Properties window (mitcad#1816): the shared input script
# tools/ui-scripts/properties-window.mitcad-ui of
# tools/ui-properties-window-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-properties-window-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/properties-window.json" --save "$MITCAD_TEST_WORK/properties-window.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/properties-window.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app properties-window --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/properties-window.mitcad"
ui_expect_log '^TestDriver: done' 'summary, merged rows, hover, details, Copy and one body'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Properties window assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Properties window'
