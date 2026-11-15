#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/InspectCommands.cpp app/commands/PropertiesReport.cpp
# check-all sources: geometry/src/sweep.cpp geometry/src/taper_events.cpp tools/cli
# A widening taper that closes a slot (mitcad#1851): the shared input
# script tools/ui-scripts/taper-closes-slot.mitcad-ui of
# tools/ui-taper-closes-slot-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-taper-closes-slot-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/taper-closes-slot.json" --save "$MITCAD_TEST_WORK/taper-closes-slot.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/taper-closes-slot.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app taper-closes-slot --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/taper-closes-slot.mitcad"
ui_expect_log '^TestDriver: done' 'the tapered extrusion closing the slot added, its volume in Properties'
ui_expect_no_log 'TestDriver: FAIL' 'no failed taper closes slot assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt taper closes slot'
