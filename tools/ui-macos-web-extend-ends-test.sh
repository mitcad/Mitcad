#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/rib.rs geometry/src/rib.cpp tools/cli
# check-all sources: tools/ui-scripts/web-extend-ends.mitcad-ui tools/ui-scripts/web-extend-ends.json
# A Web's Extend Ends and mitred corner (mitcad#1968): the shared input
# script tools/ui-scripts/web-extend-ends.mitcad-ui of
# tools/ui-web-extend-ends-test.sh in the Qt frontend: Extend Ends checked
# for a new web, the walls reaching the walls, mitred at the corner.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-web-extend-ends-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/web-extend-ends.json" --save "$MITCAD_TEST_WORK/web.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/web-extend-ends.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app web-extend-ends --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/web.mitcad"
ui_expect_log '^Added web on 2 curve\(s\), thickness 2' 'the web is added'
ui_expect_log '^TestDriver: done' 'Extend Ends is on for a new web; its walls reach the walls, mitred'
ui_expect_no_log 'TestDriver: FAIL' 'no failed web assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt web extend ends'
