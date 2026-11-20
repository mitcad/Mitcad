#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/framework/InputPicks.cpp
# check-all sources: app/framework/CommandSessionPicks.cpp app/framework/TestDriver.cpp app/commands/PatternCommands.cpp
# check-all sources: app/commands/FilletCommands.cpp tools/cli
# check-all sources: tools/ui-scripts/input-picks.mitcad-ui tools/ui-scripts/input-picks.json
# A selection input's picks listed by name on demand, each highlighted in
# the view on hover (mitcad#2064): the shared input script
# tools/ui-scripts/input-picks.mitcad-ui of tools/ui-input-picks-test.sh
# in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-input-picks-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/input-picks.json" --save "$MITCAD_TEST_WORK/input-picks.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/input-picks.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app input-picks --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/input-picks.mitcad"
ui_expect_log '^Panel Pattern on Path pick bodies hovered: 2\. Body2$' 'a row under the pointer names its pick'
ui_expect_log '^TestDriver: done' 'the picks are listed by name on demand and highlighted on hover'
ui_expect_no_log 'TestDriver: FAIL' 'no failed input picks assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt input picks'
