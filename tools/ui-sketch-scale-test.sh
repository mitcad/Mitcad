#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/sketch/SketchController.cpp app/resources/layout/default.json
# check-all sources: core/sketch core/solver core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-scale.mitcad-ui tools/ui-scripts/sketch-scale.json
# Sketch Scale (mitcad#2109) through the real UI on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-scale.json: a circle and a
# line, each dimensioned): the selected circle and line scaled by 1.5
# about the circle's center, the new radius and the dimensions read in the
# log, undone; then with Keep Values and Create Copy, the copies scaled
# and the originals with their dimensions as they were. The steps are
# tools/ui-scripts/sketch-scale.mitcad-ui, which
# tools/ui-windows-sketch-scale-test.ps1 runs on Windows and
# tools/ui-macos-sketch-scale-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-scale-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-scale.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-scale.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-scale.json" --save "$work/scale.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/scale.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Sketch Scale driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Sketch Scale sizes: c3 radius 7.500' 'the circle scaled by 1.5'
ui_expect_log 'Added Sketch Scale' 'OK kept Sketch Scale'
ui_expect_log 'TestDriver: done' 'Sketch Scale scaled, undid, and scaled copies with the values kept'
ui_finish 'UI Sketch Scale'
