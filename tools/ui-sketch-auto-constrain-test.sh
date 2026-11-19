#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/AutoConstrainCommands.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp app/sketch/SketchController.cpp
# check-all sources: app/resources/layout/default.json core/sketch core/solver core/model-api/src/sketch.rs
# check-all sources: core/model-api/src/sketch_auto_constrain.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-auto-constrain.mitcad-ui tools/ui-scripts/sketch-auto-constrain.json
# Auto Constrain (mitcad#2019) through the real UI on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-auto-constrain.json: a
# rectangle of four lines without constraints): the toolbar's button
# previews the corners joined, the lines level and upright, two lengths
# and two positions with the sketch fully constrained; with Dimensions
# unchecked the relations only; OK is one undo step. The steps are
# tools/ui-scripts/sketch-auto-constrain.mitcad-ui, which
# tools/ui-windows-sketch-auto-constrain-test.ps1 runs on Windows and
# tools/ui-macos-sketch-auto-constrain-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-auto-constrain-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-auto-constrain.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-auto-constrain.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-auto-constrain.json" --save "$work/rectangle.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/rectangle.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Auto Constrain driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Auto Constrain added: coincident 4, horizontal 2, vertical 2 (dof 4)' 'the relations only leave 4 degrees of freedom'
ui_expect_log 'Added Auto Constrain' 'OK kept Auto Constrain'
ui_expect_log 'TestDriver: done' 'Auto Constrain made the rectangle fully constrained in one undo step'
ui_finish 'UI sketch Auto Constrain'
