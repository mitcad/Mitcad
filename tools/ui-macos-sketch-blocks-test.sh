#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchBlocks.cpp app/commands/SketchCommands.cpp app/browser
# check-all sources: core/sketch core/model-api/src/sketch_blocks.rs core/model/src/document/sketch_blocks.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-blocks.mitcad-ui tools/ui-scripts/sketch-blocks.json
# Sketch blocks (mitcad#2110) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-blocks.mitcad-ui on the design mitcad-cli makes
# (tools/ui-scripts/sketch-blocks.json): Create Block of a slot, two
# instances inserted, one placed by a coincident and an angle, the block
# edited in place and every instance following, one exploded. As
# tools/ui-sketch-blocks-test.sh (Linux) and
# tools/ui-windows-sketch-blocks-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-blocks-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-blocks}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-blocks.json" --save "$MITCAD_TEST_WORK/slot.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-blocks.mitcad-ui \
  ui_run_app sketch-blocks --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/slot.mitcad"
ui_expect_log 'Created block Slot of ' 'Create Block made the slot a block in place'
ui_expect_log 'Block instance k8 of Slot: 30[.]000 mm along its direction' 'the placed instance followed the edited block'
ui_expect_log 'Exploded instance k7 of block Slot in Sketch1' 'Explode Block'
ui_expect_log 'TestDriver: done' 'sketch blocks made, inserted, edited and exploded'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch blocks; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch blocks; evidence in $UI_OUT"
