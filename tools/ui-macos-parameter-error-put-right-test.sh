#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/framework/ExpressionCheck.hpp
# check-all sources: app/framework/MessageLog.cpp app/framework/MessageBand.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowParameterEdits.cpp app/framework/TestDriver.cpp
# check-all sources: core/expr core/model-api/src/formula_help.rs core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/parameter-error-put-right.mitcad-ui
# check-all sources: tools/ui-scripts/parameter-error-put-right.json
# A parameter error put right leaves the message band, and expression
# errors in words (mitcad#2181), in the Qt frontend on macOS: the shared
# steps tools/ui-scripts/parameter-error-put-right.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/parameter-error-put-right.json). Add
# User Parameter checks the expression while it is typed and keeps OK
# disabled, a refused Expression cell says it in words in the band, the
# missing parameter added puts that error right, and the add again offers
# the near miss and adds the parameter. As
# tools/ui-parameter-error-put-right-test.sh (Linux) and
# tools/ui-windows-parameter-error-put-right-test.ps1 (Windows).
# Usage: bash tools/ui-macos-parameter-error-put-right-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-parameter-error-put-right}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/parameter-error-put-right.json" --save "$MITCAD_TEST_WORK/put-right.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/parameter-error-put-right.mitcad-ui \
  ui_run_app parameter-error-put-right --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/put-right.mitcad"
ui_expect_log 'Message band cleared [(]put right[)]' 'the error put right leaves the band'
ui_expect_log 'TestDriver: done' 'a parameter error put right leaves the band'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: parameter error put right; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: parameter error put right; evidence in $UI_OUT"
