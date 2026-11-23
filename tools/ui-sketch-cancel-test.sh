#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/MainWindowSketchCancel.cpp app/MainWindowSketch3d.cpp
# check-all sources: app/framework/DocumentSession.hpp app/sketch/SketchPalette.cpp app/sketch/Sketch3dPalette.cpp
# check-all sources: app/sketch/SketchController.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-cancel.mitcad-ui tools/ui-scripts/sketch-cancel.json
# check-all sources: app/sketch/Sketch3dController.cpp app/sketch/Sketch3dTools.cpp
# Cancel Sketch, and every sketch edit as one undo step at Finish Sketch
# (mitcad#2225), through the real UI: the shared input script
# tools/ui-scripts/sketch-cancel.mitcad-ui (also run by
# tools/ui-windows-sketch-cancel-test.ps1) on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-cancel.json): Edit Sketch on a sketch with a
# feature after it, a line and a dimension, Cancel Sketch under the
# palette: the sketch, the later feature and the timeline marker as
# before, Redo brings the edit back in one step; Create Sketch, a line,
# Cancel Sketch from the context menu: no new sketch; the same Cancel in a
# 3D sketch; Cancel with nothing changed leaves as Finish Sketch; a 3D
# sketch's edit is one undo step at Finish Sketch. Redo after Cancel
# Sketch leaves the timeline marker where Finish Sketch leaves it, and the
# app quits with a 3D sketch's Line tool running, without a crash
# (mitcad#2263).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-cancel-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-cancel.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-cancel.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-cancel.json" --save "$work/cancel.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/cancel.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Cancel driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
# The quit with the 3D Line tool running (mitcad#2263).
grep -qE "SIGSEGV|SIGABRT|terminate called|Segmentation fault" "$UI_LOG" &&
  { ui_fail 'the app crashed on its way out'; exit 1; }
ui_expect_log 'Sketch cancelled: Sketch1, ' 'Edit Sketch, a line and a dimension, Cancel Sketch: taken back'
ui_expect_log 'Redo: Edit Sketch1' 'Redo brings the line and the dimension back in one step'
ui_expect_log ', taken back, it was new' 'Create Sketch, a line, Cancel Sketch: no new sketch'
ui_expect_log 'Sketch cancelled: 3DSketch1, ' 'the same Cancel in a 3D sketch'
ui_expect_log 'Sketch cancelled: Sketch1, 0 step(s) taken back' 'nothing changed: left as Finish Sketch leaves it'
ui_expect_log '3D sketch tool Line: ends with the window' 'the app quits with the 3D Line tool running'
ui_expect_log 'TestDriver: done' 'Cancel Sketch takes the sketch edit back, Finish Sketch makes it one step'
ui_finish 'UI sketch Cancel'
