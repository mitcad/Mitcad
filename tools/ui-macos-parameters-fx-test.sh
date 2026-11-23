#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/framework/ParameterField.cpp app/framework/FormulaEditor.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-fx.mitcad-ui tools/ui-scripts/parameters-new-row.json
# Edit Parameters' Expression cells' fx (mitcad#2290): the shared input
# script tools/ui-scripts/parameters-fx.mitcad-ui of
# tools/ui-parameters-fx-test.sh in the Qt frontend, on the design that
# mitcad-cli makes (tools/ui-scripts/parameters-new-row.json): entering an
# Expression cell opens no function list; the cell's fx opens Edit Formula
# with the text typed so far on an existing row and on the empty row,
# whose OK makes the parameter; the bottom Edit Formula… uses an edited
# cell's text and works on the empty row.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-parameters-fx-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/parameters-new-row.json" --save "$MITCAD_TEST_WORK/parameters-fx.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/parameters-fx.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app parameters-fx --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/parameters-fx.mitcad"
ui_expect_log '^Formula editor opened for parameter spare with: 7$' "an existing row's fx opens Edit Formula"
ui_expect_log '^Formula editor opened for new parameter b1 with: 8$' "the empty row's fx opens Edit Formula"
ui_expect_log '^Created user parameter b1 = 9 mm \(mm\) in the new row$' "the empty row's parameter made from Edit Formula"
ui_expect_log '^Formula editor opened for parameter used with: 21$' 'Edit Formula… with the text typed in the cell'
ui_expect_log '^Formula editor for new parameter \(no name\) canceled$' 'Edit Formula… on the empty row'
ui_expect_log '^TestDriver: done' "the Expression cells' fx"
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: Qt parameters fx"
