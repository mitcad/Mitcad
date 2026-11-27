#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/MainWindow.cpp
# check-all sources: app/framework/DocumentSession.cpp app/framework/TestDriver.cpp app/framework/TestDriverModel.cpp
# check-all sources: tools/ui-scripts/sketch-busy-edits.mitcad-ui tools/ui-scripts/sketch-busy-edits.json tools/cli
# Sketch edits asked for while a job computes the model (mitcad#2500,
# GitHub mitcad/Mitcad#706), through the real UI on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-busy-edits.json: two lines with
# their lengths dimensioned): a job of the test's own holds the model
# (hold-model-job); Make Driven chosen from a dimension's context menu
# that was open as the job began, and Delete pressed on the other
# dimension, wait for the job instead of reading the document (which ended
# the application on macOS), then each is done in one undo step that one
# Undo takes back. The steps are tools/ui-scripts/sketch-busy-edits.mitcad-ui,
# which tools/ui-windows-sketch-busy-edits-test.ps1 runs on Windows and
# tools/ui-macos-sketch-busy-edits-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-busy-edits-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-busy-edits.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-busy-edits.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-busy-edits.json" --save "$work/sketch-busy-edits.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/sketch-busy-edits.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch busy edits driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sketch Make Driven waits until the model is idle' 'Make Driven waits for the held job'
grep -Eq 'Dimension k[0-9]+ made driven: ' "$UI_LOG" ||
  { ui_fail 'the dimension was not made driven once the job had ended'; exit 1; }
echo 'ok   Make Driven done once the job had ended'
ui_expect_log 'Undo: Make Driven in Sketch1' 'Make Driven after the job, one undo step'
grep -Eq '(Command sketch[.]delete|Sketch Delete) waits until the model is idle' "$UI_LOG" ||
  { ui_fail 'Delete did not wait for the held job'; exit 1; }
echo 'ok   Delete waits for the held job'
ui_expect_log 'Undo: Delete in Sketch1' 'Delete after the job, one undo step'
ui_expect_log 'TestDriver: done' 'sketch edits while a job holds the model'
ui_finish 'UI sketch edits while a job holds the model'
