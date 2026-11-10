#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/MainWindow.cpp
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document.rs core/model-api/src/lib.rs core/expr/src/table.rs
# check-all sources: tools/ui-scripts/sketch-remove-dimension-name.mitcad-ui
# check-all sources: tools/ui-scripts/sketch-remove-dimension-name.json
# A name given to a sketch dimension's parameter taken off again
# (mitcad#1577), through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-remove-dimension-name.json: a 20 x 10 rectangle,
# its width k5 d1, its height k6 d2): k5 named width = 20 mm and k6 set to
# width / 2; the name deleted in k5's value editor gives d1 back and k6's
# expression follows (d1 / 2), one undo step; Remove Name in the value
# editor's right-click menu and Remove Name width in k5's context menu do
# the same. The steps are
# tools/ui-scripts/sketch-remove-dimension-name.mitcad-ui.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-remove-dimension-name-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-remove-dimension-name.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-remove-dimension-name.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-remove-dimension-name.json" --save "$work/names.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/names.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'remove dimension name driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Dimension k5 = width = 20 mm (width)' 'k5 named width'
ui_expect_log 'Dimension k6 = width / 2 (d2)' 'k6 uses width'
ui_expect_log 'Dimension k5: name width removed, now d1 (rewritten: d2)' 'the name taken off, k6 rewritten'
ui_expect_log 'Editing dimension k6 (d1 / 2)' "k6's expression follows"
ui_expect_log 'Undo: Remove Name width' 'one undo step'
ui_expect_log 'Parameter name width removed for dimension width: 20 mm' "Remove Name in the value editor's menu"
ui_expect_log 'TestDriver: done' 'the name removed in the editor, its menu and the context menu'
ui_finish 'UI sketch dimension name removed'
