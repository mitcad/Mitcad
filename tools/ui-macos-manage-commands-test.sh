#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowFiles.cpp app/browser/BrowserController.cpp app/commands/PlasticCommands.cpp
# check-all sources: app/resources/layout/default.json tools/cli tools/ui-scripts/manage-commands.mitcad-ui
# The Manage tab's Recompute All, Units… and Plastic Rules (mitcad#1982)
# in the Qt frontend on macOS, as tools/ui-manage-commands-test.sh on
# Linux: the shared steps tools/ui-scripts/manage-commands.mitcad-ui on two
# boxes that mitcad-cli makes (tools/ui-scripts/manage-commands.json),
# each command run from its group's menu on the MANAGE tab.
# Usage: tools/ui-macos-manage-commands-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-manage-commands-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/boxes.mitcad
"$cli" run "$here/ui-scripts/manage-commands.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run manage-commands.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/manage-commands.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app manage-commands --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Recompute All: 2 features, no undo step$' 'Recompute All from Manage › Design'
ui_expect_log '^Units dialog opened: mm' 'Units… from Manage › Design'
ui_expect_log '^Added plastic feature \(Plastic Rule\)$' 'Plastic Rules from Manage › Rules'
ui_expect_log '^TestDriver: done' 'the Manage tab runs Recompute All, Units… and Plastic Rules'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Manage commands assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Manage commands'
