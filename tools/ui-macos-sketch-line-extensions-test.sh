#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/solver core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-line-extensions.mitcad-ui tools/ui-scripts/sketch-line-extensions.json
# Resting the pointer on a sketch line acquires it, and a drawing tool
# snaps onto its extension and to where two acquired lines' extensions
# meet (mitcad#2035, GitHub mitcad/Mitcad#433) in the Qt frontend on macOS:
# the shared steps tools/ui-scripts/sketch-line-extensions.mitcad-ui on the
# design mitcad-cli makes (tools/ui-scripts/sketch-line-extensions.json): a
# line ended on a vertical line's extension is held there by a Coincident
# and follows the line when it moves; a rectangle's corner where two
# acquired lines' extensions meet is held on both; with Snap to line
# extensions unchecked nothing is acquired. As
# tools/ui-sketch-line-extensions-test.sh (Linux) and
# tools/ui-windows-sketch-line-extensions-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-line-extensions-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-line-extensions}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-line-extensions.json" --save "$MITCAD_TEST_WORK/lines.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-line-extensions.mitcad-ui \
  ui_run_app sketch-line-extensions --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/lines.mitcad"
ui_expect_log 'on the extension of c2' 'a line end held on a line extension'
ui_expect_log 'on the extension of c8' 'a rectangle corner held where two line extensions meet'
ui_expect_log 'TestDriver: done' 'line extensions acquired, snapped to, followed and turned off'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch line extensions; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch line extensions; evidence in $UI_OUT"
