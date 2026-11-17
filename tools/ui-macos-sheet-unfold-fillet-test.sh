#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# check-all sources: tools/ui-scripts/sheet-unfold-fillet.mitcad-ui tools/ui-scripts/sheet-unfold-fillet.json
# An Unfold refused by a fillet on a surface edge (mitcad#1942): the shared
# input script tools/ui-scripts/sheet-unfold-fillet.mitcad-ui of
# tools/ui-sheet-unfold-fillet-test.sh in the Qt frontend: the edited
# Unfold's panel says why in words, the fillet by its name, no ids.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-unfold-fillet-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-unfold-fillet.json" --save "$MITCAD_TEST_WORK/unfold.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-unfold-fillet.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-unfold-fillet --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/unfold.mitcad"
ui_expect_log '^Panel Unfold error: sheet Unfold needs the sheet' 'the Unfold panel says why it fails'
ui_expect_log '^TestDriver: done' 'the Unfold panel names the fillet in words, without ids'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Unfold message assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Unfold message names the fillet in words'
