#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowArrivals.cpp app/files/MainWindowFiles.cpp app/webbrowser/MainWindowComponentWeb.cpp
# check-all sources: app/framework/CommandSessionVisibility.cpp app/framework/TestDriverFiles.cpp app/commands/InspectCommands.cpp tools/cli
# Work that arrives while a command is open (mitcad#2281) in the Qt frontend
# on macOS, as tools/ui-insert-during-command-test.py on Linux and Windows:
# a Component from Web download (its test command) and a STEP file dropped
# on the 3D view come in at once beside Measure, which stays open, and wait
# while Extrude is open, coming in after its Cancel.
# Usage: tools/ui-macos-insert-during-command-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-insert-during-command-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 "$here/ui-insert-during-command-test.py" prepare "$2" "$MITCAD_TEST_WORK" || exit 1
UI_TIMEOUT=240
MITCAD_TEST_WEB_FILE=$MITCAD_TEST_WORK/PA100_pin.step MITCAD_WEB_DOWNLOADS_DIR=$MITCAD_TEST_WORK/downloads \
  MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/insert-during-command.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app insert-during-command --ui=qt --no-native-dialogs --no-recovery --no-mcp \
  --open "$MITCAD_TEST_WORK/design.mitcad"
ui_expect_log '^TestDriver: quit' 'downloads and drops beside Measure and after Extrude'
ui_expect_no_log 'TestDriver: FAIL' 'all insert-during-command assertions passed'
[ "$UI_FAILURES" = 0 ] || exit 1
python3 "$here/ui-insert-during-command-test.py" verify "$MITCAD_TEST_WORK" || exit 1
echo 'PASS: Qt downloads and drops while a command is open'
