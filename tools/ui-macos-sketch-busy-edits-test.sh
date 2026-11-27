#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/MainWindow.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverModel.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-busy-edits.mitcad-ui tools/ui-scripts/sketch-busy-edits.json
# Sketch edits asked for while a job computes the model (mitcad#2500,
# GitHub mitcad/Mitcad#706: Make Driven and Delete on a dimension ended
# the application with an uncaught ModelBusy on macOS) in the Qt frontend
# on macOS, as tools/ui-sketch-busy-edits-test.sh on Linux: the shared
# steps tools/ui-scripts/sketch-busy-edits.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-busy-edits.json). While a job
# of the test's own holds the model, Make Driven from a dimension's context
# menu and Delete on another dimension wait for it, the application stays
# open, and each is done in one undo step once the job has ended.
# Usage: tools/ui-macos-sketch-busy-edits-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sketch-busy-edits-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/sketch-busy-edits.mitcad
"$cli" run "$here/ui-scripts/sketch-busy-edits.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run sketch-busy-edits.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-busy-edits.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketch-busy-edits --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Sketch Make Driven waits until the model is idle$' 'Make Driven waits for the held job'
ui_expect_log '^Undo: Make Driven in Sketch1$' 'Make Driven after the job, one undo step'
ui_expect_log '^Undo: Delete in Sketch1$' 'Delete after the job, one undo step'
ui_expect_log '^TestDriver: done' 'sketch edits while a job holds the model'
ui_expect_no_log 'TestDriver: FAIL' 'no failed sketch busy edit assertion'
ui_expect_no_log 'uncaught exception' 'no uncaught ModelBusy'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt sketch edits while a job holds the model'
