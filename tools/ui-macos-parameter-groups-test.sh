#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp tools/cli
# check-all sources: tools/ui-scripts/parameter-groups.mitcad-ui tools/ui-scripts/parameter-groups.json
# Groups of user parameters in Edit Parameters (mitcad#2180): the shared
# input script tools/ui-scripts/parameter-groups.mitcad-ui of
# tools/ui-parameter-groups-test.sh in the Qt frontend, on the design that
# mitcad-cli makes (tools/ui-scripts/parameter-groups.json): a group made,
# two parameters moved into it by the context menu, the group renamed in
# place, collapsed (kept when the dialog opens again) and deleted, its
# parameters back under User Parameters.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-parameter-groups-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/parameter-groups.json" --save "$MITCAD_TEST_WORK/parameter-groups.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/parameter-groups.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app parameter-groups --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/parameter-groups.mitcad"
ui_expect_log '^Parameter group added: Cabinet$' 'New Group makes a group'
ui_expect_log '^Parameters moved to group Cabinet: width, depth$' 'the context menu moves two parameters into it'
ui_expect_log '^Parameter group Cabinet renamed to Table$' 'the group is renamed in place'
ui_expect_log '^Parameters group Table collapsed$' 'the group collapses'
ui_expect_log '^Parameter group Table deleted: width, depth moved to User Parameters$' 'a deleted group leaves its parameters'
ui_expect_log '^TestDriver: done' 'the parameters are grouped, moved, renamed, collapsed and ungrouped'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt parameter groups'
