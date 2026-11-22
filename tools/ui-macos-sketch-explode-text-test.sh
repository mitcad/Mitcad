#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: core/sketch core/model/src/document/sketch_explode_text.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-explode-text.mitcad-ui tools/ui-scripts/sketch-explode-text.json
# Explode Text (mitcad#2163) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-explode-text.mitcad-ui on the design mitcad-cli
# makes (tools/ui-scripts/sketch-explode-text.json): the text right-clicked,
# Explode Text from its context menu, then Trim on a piece of the letter.
# As tools/ui-sketch-explode-text-test.sh (Linux) and
# tools/ui-windows-sketch-explode-text-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-explode-text-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-explode-text}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-explode-text.json" --save "$MITCAD_TEST_WORK/text.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-explode-text.mitcad-ui \
  ui_run_app sketch-explode-text --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/text.mitcad"
ui_expect_log 'Exploded text t1 in Sketch1: ' 'Explode Text from the context menu'
ui_expect_log 'Trimmed c4 at ' 'Trim on a piece of the letter'
ui_expect_log 'TestDriver: done' 'a text exploded and a piece of it trimmed'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch explode text; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch explode text; evidence in $UI_OUT"
