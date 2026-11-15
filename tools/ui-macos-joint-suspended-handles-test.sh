#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp tools/cli tools/ui-scripts/joint-suspended-handles.mitcad-ui
# A command panel set aside hides its handles (mitcad#1844): the shared input script
# tools/ui-scripts/joint-suspended-handles.mitcad-ui of tools/ui-joint-suspended-handles-test.sh in
# the Qt frontend, on the design that mitcad-cli makes
# (tools/ui-scripts/joint-suspended-handles.json).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-joint-suspended-handles-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/joint-suspended-handles.json" --save "$MITCAD_TEST_WORK/joint-suspended-handles.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/joint-suspended-handles.mitcad-ui MITCAD_TEST_STEP_MS=100   ui_run_app joint-suspended-handles --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/joint-suspended-handles.mitcad"
ui_expect_log '^TestDriver: done' 'the set-aside panel handles hidden, back on resume'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt joint-suspended-handles'
