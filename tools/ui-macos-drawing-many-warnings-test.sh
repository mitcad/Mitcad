#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/CommandDockWidth.cpp app/framework/HangWatchdog.cpp tools/cli
# check-all sources: app/framework/TestDriver.cpp app/browser/BrowserController.cpp
# check-all sources: tools/ui-scripts/drawing-many-warnings.mitcad-ui tools/ui-scripts/drawing-many-warnings.json
# A base view with many warnings (mitcad#2507, follow-up of mitcad#2476;
# GitHub mitcad/Mitcad#694): the shared input script
# tools/ui-scripts/drawing-many-warnings.mitcad-ui of
# tools/ui-drawing-many-warnings-test.sh in the Qt frontend, Docked, the
# hang watchdog on at 15 s: the view selected says its warning once with
# its count and the drawing's area keeps its size, Close Drawing shows the
# 3D view again, the drawing opened again and its view selected, with no
# "UI not responding" line all along.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-many-warnings-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-many-warnings.json" --save "$MITCAD_TEST_WORK/plates.mitcad" || exit 1
# The hang watchdog on, with room for the slow steps of a renderer without
# a GPU: the window that stopped answering did so for minutes.
MITCAD_HANG_WATCHDOG_MS=15000 MITCAD_TEST_INPUT=$here/ui-scripts/drawing-many-warnings.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-many-warnings --ui=qt --chrome=docked --no-recovery --no-mcp --no-native-dialogs \
  --open "$MITCAD_TEST_WORK/plates.mitcad"
ui_expect_no_log '[hang watchdog]' 'no line of the hang watchdog'
ui_expect_log '^Drawing view 1 panel warnings \(49 of them\): an occurrence is gone \(49 times\)$' \
  'the warning said once, with how many times it came'
ui_expect_log '^Drawing closed$' 'Close Drawing showed the 3D view again'
ui_expect_log '^TestDriver: done' 'the drawing opened again, its view selected, the window answering'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing view with many warnings'
