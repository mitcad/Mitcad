#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/plane_geometry.rs core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-body-snaps.mitcad-ui tools/ui-scripts/sketch-body-snaps.json
# Every drawing tool snaps to body corners and edges in the sketch plane
# (mitcad#1967, GitHub mitcad/Mitcad#392) in the Qt frontend on macOS: the
# shared steps tools/ui-scripts/sketch-body-snaps.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/sketch-body-snaps.json: a 60 x 40 x 20
# block, Sketch2 on its top face): a line from a block corner and a
# rectangle's corner on another are held on the corners' linked
# projections (Coincident); a line started on a block edge away from its
# middle is held on the edge's linked projection. As
# tools/ui-sketch-body-snaps-test.sh (Linux) and
# tools/ui-windows-sketch-body-snaps-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-body-snaps-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-body-snaps}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-body-snaps.json" --save "$MITCAD_TEST_WORK/block.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-body-snaps.mitcad-ui \
  ui_run_app sketch-body-snaps --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/block.mitcad"
ui_expect_log ', the projected corner V[{]' 'a drawn point held on a projected block corner'
ui_expect_log ', the projected edge E[{]' 'a line start held on a projected block edge'
ui_expect_log 'TestDriver: done' 'a line and a rectangle from block corners, a line from a block edge'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch body snaps; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch body snaps; evidence in $UI_OUT"
