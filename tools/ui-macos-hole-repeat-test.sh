#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/Command.hpp app/commands/HoleSizes.cpp
# check-all sources: tools/cli tools/ui-scripts/hole-repeat.mitcad-ui
# Repeat runs a value's reaction with the repeated value (mitcad#1844) and
# keeps typed only what was typed (mitcad#1845): the shared input script
# tools/ui-scripts/hole-repeat.mitcad-ui of tools/ui-hole-repeat-test.sh in
# the Qt frontend, on the design that mitcad-cli makes
# (tools/ui-scripts/hole-repeat.json).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-hole-repeat-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/hole-repeat.json" --save "$MITCAD_TEST_WORK/hole-repeat.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/hole-repeat.mitcad-ui MITCAD_TEST_STEP_MS=100   ui_run_app hole-repeat --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/hole-repeat.mitcad"
ui_expect_log '^TestDriver: done' 'the sizes follow the thread, a typed one stays typed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt hole-repeat'
