#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/framework/ExpressionCheck.hpp
# check-all sources: app/framework/MessageLog.cpp app/framework/MessageBand.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowParameterEdits.cpp app/framework/TestDriver.cpp
# check-all sources: core/expr core/model-api/src/formula_help.rs core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/parameter-error-put-right.mitcad-ui
# check-all sources: tools/ui-scripts/parameter-error-put-right.json
# A parameter error put right leaves the message band, and expression
# errors in words (mitcad#2181), the shared input script
# tools/ui-scripts/parameter-error-put-right.mitcad-ui (also run by
# tools/ui-windows-parameter-error-put-right-test.ps1 and
# tools/ui-macos-parameter-error-put-right-test.sh): the Parameters
# window's empty row checks the expression while it is typed, a refused
# Expression cell says it in words in the band, the missing parameter
# added puts that error right (the band shows nothing of it), and the add
# again offers the near miss and adds the parameter.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameter-error-put-right-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-parameter-error-put-right.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/parameter-error-put-right.mitcad-ui
"$CLI" run "$here/ui-scripts/parameter-error-put-right.json" --save "$WORK/put-right.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/put-right.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'parameter error put right driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Parameter cabinet_lenght expression problem while typed: table_lenght is not a parameter (marked 0..12)' \
  'the empty row checks the expression while it is typed'
ui_expect_log 'Message band full: error The expression of depth uses table_lenght, which is not a parameter.' \
  'a refused change in words in the band'
ui_expect_log 'Message band cleared (put right)' 'the error put right leaves the band'
ui_expect_log 'Did you mean table_lenght?' 'a near miss offered'
ui_expect_log 'Created user parameter cabinet_lenght = (table_lenght+sheet_thickness) (mm) in the new row' 'the add again'
ui_expect_log 'TestDriver: done' 'a parameter error put right leaves the band'
ui_finish 'UI parameter error put right'
