#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/resources/layout/default.json
# check-all sources: core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# check-all sources: tools/ui-scripts/sheet-bead.mitcad-ui tools/ui-scripts/sheet-bead.json
# The Bead command (mitcad#2645): the shared input script
# tools/ui-scripts/sheet-bead.mitcad-ui of tools/ui-sheet-bead-test.sh in the
# Qt frontend: a round bead pressed along a sketch line, changed to a
# trapezoid and pressed into the face.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sheet-bead-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sheet-bead.json" --save "$MITCAD_TEST_WORK/bead.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sheet-bead.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sheet-bead --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/bead.mitcad"
ui_expect_log '^Editing F5 with Bead' 'the bead opened in Bead'
ui_expect_log '^TestDriver: done' 'a bead made, changed to a trapezoid and pressed into the face'
ui_expect_no_log 'TestDriver: FAIL' 'no failed bead assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Bead command'
