#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/framework/DocumentSession.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchPreferences.cpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-esc-leaves.mitcad-ui tools/ui-scripts/sketch-esc-leaves.json
# Esc leaves a sketch nothing was changed in (mitcad#1623) through the real
# UI, the shared input script tools/ui-scripts/sketch-esc-leaves.mitcad-ui
# (also run by tools/ui-windows-sketch-esc-leaves-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-esc-leaves.json): with the
# preference "Esc leaves an unchanged sketch" off (the default), Esc in a
# new sketch leaves nothing; with it on, Create Sketch
# and Esc leaves no sketch and no undo step; Edit Sketch and Esc leaves the
# sketch as it was; a line drawn keeps the sketch open on Esc, with a hint
# once; Ctrl+Z on the line, and Esc leaves it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-esc-leaves-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-esc-leaves.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-esc-leaves.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-esc-leaves.json" --save "$work/esc.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/esc.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Esc driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: setting sketch/escLeavesUnchangedSketch = true' 'the preference off first (Esc left nothing), then turned on'
ui_expect_log ' taken back, it was new and empty' 'Create Sketch, Esc: the new empty sketch taken back'
ui_expect_log 'Esc: leaving Sketch1, nothing changed in it' 'Edit Sketch, Esc: the unchanged sketch left'
ui_expect_log 'Esc: Sketch1 changed, it stays open; hint: ' 'a line drawn: Esc keeps the sketch open and says how to leave it'
[ "$(grep -c '^Esc: Sketch1 changed, it stays open; hint: ' "$UI_LOG")" = 1 ] ||
  ui_fail 'the hint on leaving a changed sketch was given more than once'
echo 'ok   the hint given once'
ui_expect_log 'TestDriver: done' 'Esc leaves an unchanged sketch and keeps a changed one'
ui_finish 'UI sketch Esc leaves'
