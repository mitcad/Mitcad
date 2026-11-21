#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-show-unconstrained.mitcad-ui
# check-all sources: tools/ui-scripts/sketch-show-unconstrained.json
# Show Unconstrained (mitcad#2026, GitHub mitcad/Mitcad#413), the shared
# input script tools/ui-scripts/sketch-show-unconstrained.mitcad-ui (also
# run by app.ui-windows-sketch-show-unconstrained and
# app.ui-macos-sketch-show-unconstrained) on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-show-unconstrained.json): a sketch with one free
# line and two fully constrained ones; the Sketch Palette's Show
# Unconstrained button, a click on the degrees of freedom badge and the
# sketch's context menu each select exactly the free line and its two
# points.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-show-unconstrained-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-show-unconstrained.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-show-unconstrained.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-show-unconstrained.json" --save "$work/unconstrained.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/unconstrained.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch show unconstrained driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Palette: Show Unconstrained' 'the palette button ran Show Unconstrained'
ui_expect_log 'Sketch badge clicked' 'a click on the degrees of freedom badge ran it'
ui_expect_log 'Show Unconstrained: 1 curve(s), 2 point(s): ' 'the free line and its two points selected'
ui_expect_log 'TestDriver: done' 'Show Unconstrained from the palette, the badge and the context menu'
ui_finish 'UI sketch show unconstrained'
