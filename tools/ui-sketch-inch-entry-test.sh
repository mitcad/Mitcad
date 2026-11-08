#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/DrawTools.cpp app/browser/ParametersDialog.cpp
# check-all sources: core/model-base/src/values.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-inch-entry.mitcad-ui tools/ui-scripts/sketch-inch-entry.json
# check-all sources: core/expr/src/lexer.rs core/expr/src/parser.rs
# check-all sources: tools/ui-scripts/sketch-inch-default-units.mitcad-ui app/MainWindowStart.cpp
# check-all sources: app/view/ViewController.cpp app/files/StartPage.cpp app/WindowManager.cpp
# Lengths typed in a design in inches are stored as inches (mitcad#1452,
# GitHub mitcad/Mitcad#187), through the real UI on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-inch-entry.json: Sketch1 made
# in mm with a circle dimensioned 20 mm, then the design set to inches):
# Add User Parameter starts in inches, the mm dimension set to 0.25 * 2 is
# 12.7 mm, a rectangle typed 0.875 by 12.125 is 22.225 x 307.975 mm and a
# circle typed 0.2 is 5.08 mm, each read back from the model; then, the
# design set to mm, a circle typed with the inch mark, .875", is 22.225 mm
# (mitcad#1531). The steps
# are tools/ui-scripts/sketch-inch-entry.mitcad-ui, which
# tools/ui-windows-sketch-inch-entry-test.ps1 runs on Windows.
# Then the reported sequence on a fresh start with the Start page shown
# (tools/ui-scripts/sketch-inch-default-units.mitcad-ui): Default units for
# new designs set to in, the empty untitled design starts again in inches
# and a rectangle typed .875 by 12.25 is 22.225 x 311.15 mm.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-inch-entry-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-inch-entry.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-inch-entry.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-inch-entry.json" --save "$work/inch.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/inch.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch values in inches driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Add User Parameter dialog opened: unit in' 'a new parameter starts in inches'
ui_expect_log 'Added rectangle 22.22' 'the rectangle typed 0.875 by 12.125 in'
ui_expect_log 'Added circle, diameter 5.0' 'the circle typed 0.2 in'
ui_expect_log 'Added circle, diameter 22.22' 'the circle typed .875" in a design in mm'
ui_expect_log 'TestDriver: done' 'typed values, a dimension made in mm and a new parameter, all stored as inches'

# The reported sequence (tools/ui-scripts/sketch-inch-default-units.mitcad-ui):
# a fresh start on the Start page, the default units set to in, the
# untitled design drawn in by typing.
export MITCAD_START_PAGE=1
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-inch-default-units.mitcad-ui
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'default units driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'The untitled design starts again in in' 'the empty untitled design takes the new default units'
ui_expect_log 'Start page new design units: in' "the Start page's New Design card says in"
ui_expect_log 'Added rectangle 22.22' 'the rectangle typed .875 by 12.25 after the preference'
ui_expect_log 'TestDriver: done' 'the default units set after the start, then drawn by typing, stored as inches'
ui_finish 'UI sketch values typed in a design in inches'
