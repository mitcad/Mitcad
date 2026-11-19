#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/AutoConstrainCommands.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/sketch/SketchController.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-auto-constrain.mitcad-ui tools/ui-scripts/sketch-auto-constrain.json
# Auto Constrain (mitcad#2019) in the Qt frontend on macOS: the shared
# steps tools/ui-scripts/sketch-auto-constrain.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/sketch-auto-constrain.json): a
# rectangle of four lines without constraints ends fully constrained, the
# relations only leave 4 degrees of freedom, and OK is one undo step. As
# tools/ui-sketch-auto-constrain-test.sh (Linux) and
# tools/ui-windows-sketch-auto-constrain-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-auto-constrain-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-auto-constrain}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-auto-constrain.json" --save "$MITCAD_TEST_WORK/rectangle.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-auto-constrain.mitcad-ui \
  ui_run_app sketch-auto-constrain --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/rectangle.mitcad"
ui_expect_log 'Preview Auto Constrain added: coincident 4, horizontal 2, vertical 2 [(]dof 4[)]' 'the relations only leave 4 degrees of freedom'
ui_expect_log 'Added Auto Constrain' 'OK kept Auto Constrain'
ui_expect_log 'TestDriver: done' 'Auto Constrain made the rectangle fully constrained in one undo step'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Auto Constrain; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Auto Constrain; evidence in $UI_OUT"
