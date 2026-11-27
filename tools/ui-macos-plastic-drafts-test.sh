#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PlasticCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/plastic.rs geometry/src/plastic.cpp geometry/src/rib.cpp tools/cli
# check-all sources: tools/ui-scripts/plastic-drafts.mitcad-ui tools/ui-scripts/plastic-drafts.json
# Plastic Feature's fields of mitcad#2511: the shared input script
# tools/ui-scripts/plastic-drafts.mitcad-ui of tools/ui-plastic-drafts-test.sh
# in the Qt frontend: a lip with Follow Tangency and a Side Draft of its own,
# a boss with a rib up to the wall.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-plastic-drafts-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/plastic-drafts.json" --save "$MITCAD_TEST_WORK/plastic.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/plastic-drafts.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app plastic-drafts --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plastic.mitcad"
ui_expect_log '^Added plastic feature \(Lip\)' 'the lip is added'
ui_expect_log '^Added plastic feature \(Boss\)' 'the boss is added'
ui_expect_log '^TestDriver: done' 'the lip follows the face beside it and leans; a rib reaches the wall'
ui_expect_no_log 'TestDriver: FAIL' 'no failed plastic assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt plastic drafts'
