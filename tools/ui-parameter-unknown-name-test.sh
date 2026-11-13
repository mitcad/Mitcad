#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/CommandPanel.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/DrawTools.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/parameter-unknown-name.mitcad-ui
# check-all sources: tools/ui-scripts/parameter-unknown-name.json
# An unknown name and Enter make a user parameter in every value field
# (mitcad#1723), through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/parameter-unknown-name.json: Sketch1 with a 40 x 25
# rectangle and a free line, Extrude1 10 mm of the rectangle): the
# Rectangle tool's Width, a new sketch dimension and Extrude1's Distance
# each open Add User Parameter for a name typed and Enter, and the new
# parameter is a user parameter; side = 18 mm typed in a dimension's value
# editor still names that dimension's own parameter. The steps are
# tools/ui-scripts/parameter-unknown-name.mitcad-ui, which
# tools/ui-windows-parameter-unknown-name-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameter-unknown-name-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-parameter-unknown-name.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/parameter-unknown-name.mitcad-ui
"$cli" run "$here/ui-scripts/parameter-unknown-name.json" --save "$work/unknown.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/unknown.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'unknown name driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Add User Parameter opened for sketch width: name rect_side' "a drawing tool's field: Add User Parameter"
ui_expect_log 'Created user parameter rect_side = 10 mm (mm) for sketch width' 'rect_side made'
ui_expect_log 'Add User Parameter opened for sketch dimension: name line_len' 'a sketch dimension: Add User Parameter'
ui_expect_log 'Created user parameter line_len = 25 mm (mm) for sketch dimension' 'line_len made'
ui_expect_log 'Dimension k5 = side = 18 mm (side)' 'name = value still names the dimension'
ui_expect_log 'Add User Parameter opened for Extrude Distance: name ext_height' "a panel's field: Add User Parameter"
ui_expect_log 'Created user parameter ext_height = 12 mm (mm) for Extrude Distance' 'ext_height made'
ui_expect_log 'TestDriver: done' 'an unknown name and Enter make a user parameter in every field'
ui_finish 'UI parameter unknown name'
