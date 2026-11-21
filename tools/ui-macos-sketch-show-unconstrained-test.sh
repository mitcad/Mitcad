#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# Show Unconstrained (mitcad#2026) on macOS in Qt, as
# tools/ui-sketch-show-unconstrained-test.sh on Linux: the shared input
# script tools/ui-scripts/sketch-show-unconstrained.mitcad-ui, the Sketch
# Palette's button, a click on the degrees of freedom badge and the
# sketch's context menu each selecting exactly the free line and its two
# points.
# Usage: tools/ui-macos-sketch-show-unconstrained-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sketch-show-unconstrained-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-show-unconstrained.json" --save "$MITCAD_TEST_WORK/unconstrained.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-show-unconstrained.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketch-show-unconstrained --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/unconstrained.mitcad"
ui_expect_log '^Palette: Show Unconstrained$' 'the palette button ran Show Unconstrained'
ui_expect_log '^Sketch badge clicked$' 'a click on the degrees of freedom badge ran it'
ui_expect_log '^Show Unconstrained: 1 curve\(s\), 2 point\(s\): ' 'the free line and its two points selected'
ui_expect_log '^TestDriver: done' 'Show Unconstrained from the palette, the badge and the context menu'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt sketch show unconstrained'
