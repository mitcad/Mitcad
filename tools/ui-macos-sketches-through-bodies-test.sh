#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/ViewController.cpp app/browser/BrowserController.cpp
# check-all sources: app/MainWindowVisibility.cpp app/framework/TestDriver.cpp tools/cli
# Sketches through the bodies outside sketch mode (mitcad#1931) on macOS in
# Qt, as tools/ui-sketches-through-bodies-test.sh on Linux: the shared input
# script tools/ui-scripts/sketches-through-bodies.mitcad-ui on the design
# tools/ui-scripts/sketches-through-bodies.json makes: the profile a block
# hides drawn through it, picked in Extrude and joined to the block; Show
# All Sketches, Sketches Through Bodies off and on, Hide All Sketches and
# its undo.
# Usage: tools/ui-macos-sketches-through-bodies-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sketches-through-bodies-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketches-through-bodies.json" --save "$MITCAD_TEST_WORK/through.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sketches-through-bodies.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketches-through-bodies --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/through.mitcad"
ui_expect_log '^Sketches through the bodies: F2 4 curve' 'the finished sketch and its profile drawn through the block'
ui_expect_log '^Added extrude \(Join\)' 'the hidden profile picked and joined to the block'
ui_expect_log '^All sketches Root: hidden \(1\)' 'Hide All Sketches hides it'
ui_expect_log '^TestDriver: done' 'sketches through the bodies outside sketch mode'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt sketches through bodies'
