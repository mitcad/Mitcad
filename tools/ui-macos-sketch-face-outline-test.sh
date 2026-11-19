#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp app/MainWindow.cpp app/view tools/cli
# check-all sources: core/model-api/src/sketch.rs core/sketch/src/regions.rs core/sketch/src/def.rs
# check-all sources: tools/ui-scripts/sketch-face-outline.mitcad-ui tools/ui-scripts/sketch-face-outline.json
# A sketch on a planar face starts with the face's outline, and Convert to
# Sketch Geometry takes a profile (mitcad#2051, GitHub mitcad/Mitcad#426),
# in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-face-outline.mitcad-ui on the design mitcad-cli
# makes (tools/ui-scripts/sketch-face-outline.json: a 60 x 40 x 20 block).
# As tools/ui-sketch-face-outline-test.sh (Linux) and
# tools/ui-windows-sketch-face-outline-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-face-outline-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-face-outline}"
# The preference's own default, on (the library turns it off).
export MITCAD_SKETCH_PROJECT_FACE=1
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-face-outline.json" --save "$MITCAD_TEST_WORK/block.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-face-outline.mitcad-ui \
  ui_run_app sketch-face-outline --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/block.mitcad"
ui_expect_log "starts with the face's outline: " 'a new sketch on a face starts with its outline'
ui_expect_log 'Undo: Convert to Sketch Geometry in Sketch2' 'the converted outline restored by one undo'
ui_expect_log 'Preferences: project the face' 'the preference turned off'
ui_expect_log 'TestDriver: done' 'outline, Convert of its profile, no Convert of ordinary curves, the preference off'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch face outline; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch face outline; evidence in $UI_OUT"
