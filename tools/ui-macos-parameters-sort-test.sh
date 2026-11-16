#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/ParameterSort.cpp app/framework/ParameterUnits.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-sort.mitcad-ui tools/ui-scripts/parameters-sort.json
# Edit Parameters sorted by a column (mitcad#1866): the shared input script
# tools/ui-scripts/parameters-sort.mitcad-ui of tools/ui-parameters-sort-test.sh
# in the Qt frontend, on the design that mitcad-cli makes
# (tools/ui-scripts/parameters-sort.json): Name's and Value's headers sort
# the rows within their groups, both ways and back to the creation order;
# the choice is kept; an edited and an added row stay selected in their
# place.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-parameters-sort-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/parameters-sort.json" --save "$MITCAD_TEST_WORK/parameters-sort.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/parameters-sort.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app parameters-sort --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/parameters-sort.mitcad"
ui_expect_log '^Parameters sorted by name ascending$' "Name's header sorts the rows"
ui_expect_log '^Parameters sorted in creation order$' 'a third click goes back to the creation order'
ui_expect_log '^TestDriver: done' 'the rows sort within their groups by a column'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt parameters sort'
