#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser/BrowserController.cpp app/MainWindowSheet.cpp
# check-all sources: app/framework/CommandRegistry.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/drawing-flat-pattern-commands.mitcad-ui tools/ui-scripts/drawing-flat-pattern-commands.json
# The Flat Pattern's drawing commands (mitcad#2446): the shared input script
# tools/ui-scripts/drawing-flat-pattern-commands.mitcad-ui of
# tools/ui-drawing-flat-pattern-commands-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-drawing-flat-pattern-commands-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/drawing-flat-pattern-commands.json" --save "$MITCAD_TEST_WORK/plate.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/drawing-flat-pattern-commands.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app drawing-flat-pattern-commands --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plate.mitcad"
ui_expect_log '^Drawing tool: Flat Pattern View' 'Flat Pattern View from the Create menu'
ui_expect_log '^Drawing Flat Pattern placed: view 1 of F3.b0' 'Create Drawing of Flat Pattern from the Flat Pattern row'
ui_expect_log '^Drawing Flat Pattern views: view 1: Plate, 50.65 x 20.00, 1 bend line' 'the blank drawn with its bend line'
ui_expect_log '^TestDriver: done' 'both commands placed the Flat Pattern'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Flat Pattern command assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing Flat Pattern commands'
