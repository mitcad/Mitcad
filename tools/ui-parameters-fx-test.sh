#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/ParametersDialog.hpp
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp app/framework/FormulaEditor.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-fx.mitcad-ui tools/ui-scripts/parameters-new-row.json
# Edit Parameters' Expression cells' fx (mitcad#2290) through the real UI,
# the shared input script tools/ui-scripts/parameters-fx.mitcad-ui (also
# run by the Windows and macOS wrappers) on the design that mitcad-cli
# makes (tools/ui-scripts/parameters-new-row.json):
#   - entering an Expression cell, of an existing row and of the empty
#     row, opens no function list;
#   - the cell's fx opens Edit Formula with the text typed so far, never
#     the list; in the empty row OK makes the parameter with the window's
#     formula;
#   - the bottom Edit Formula… uses the text typed in an edited cell, and
#     on the empty row opens Edit Formula for a new parameter (Cancel makes
#     nothing).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameters-fx-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-parameters-fx.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/parameters-fx.mitcad-ui
"$cli" run "$here/ui-scripts/parameters-new-row.json" --save "$work/parameters-fx.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/parameters-fx.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the parameters fx driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail 'the parameters fx driver failed'; exit 1; }
ui_expect_log 'Formula editor opened for parameter spare with: 7' "an existing row's fx opens Edit Formula"
ui_expect_log 'Formula editor opened for new parameter b1 with: 8' "the empty row's fx opens Edit Formula"
ui_expect_log 'Created user parameter b1 = 9 mm (mm) in the new row' "the empty row's parameter made from Edit Formula"
ui_expect_log 'Formula editor opened for parameter used with: 21' "Edit Formula… with the text typed in the cell"
ui_expect_log 'Formula editor for new parameter (no name) canceled' "Edit Formula… on the empty row"
ui_expect_log 'TestDriver: done' "the Expression cells' fx"
ui_finish "UI Parameters window: the Expression cells' fx opens Edit Formula"
