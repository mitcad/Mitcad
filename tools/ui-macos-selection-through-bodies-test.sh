#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/ViewController.cpp app/PickContext.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# The selection through the bodies (mitcad#1956) on macOS in Qt, as
# tools/ui-selection-through-bodies-test.sh on Linux: the shared input
# script tools/ui-scripts/selection-through-bodies.mitcad-ui on the design
# tools/ui-scripts/selection-through-bodies.json makes: the profile a block
# hides, selected with no command active, and a face of the block turned
# away drawn through it; with Selection Through Bodies off, neither.
# Usage: tools/ui-macos-selection-through-bodies-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-selection-through-bodies-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/selection-through-bodies.json" --save "$MITCAD_TEST_WORK/through.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/selection-through-bodies.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app selection-through-bodies --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/through.mitcad"
ui_expect_log '^Selection through the bodies: profile F2/' 'the buried profile drawn through the block'
ui_expect_log '^Selection through the bodies: face F1\.b0/' 'the face turned away drawn through the block'
ui_expect_log '^Selection through the bodies: off$' 'Selection Through Bodies off: neither'
ui_expect_log '^TestDriver: done' 'the selection through the bodies'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt selection through bodies'
