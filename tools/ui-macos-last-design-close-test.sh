#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/WindowManager.cpp app/DocumentFrame.cpp app/MainWindowStart.cpp app/files/MainWindowFiles.cpp
# check-all sources: app/framework/AppSettings.cpp app/view/ViewController.cpp app/framework/TestDriverDocuments.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/last-design-close.mitcad-ui
# check-all sources: tools/ui-scripts/last-design-close-restart.mitcad-ui tools/ui-scripts/panel-message-top.json
# Closing the last design (mitcad#2142) on macOS, in the Qt frontend, the
# same scripts as the Linux and Windows tests' but the tab's × (macOS shows
# no document tab for one design): tools/ui-scripts/last-design-close.mitcad-ui
# in the Floating layout (Preferences › General › Quit MitCad when the last
# design is closed off, the default: File › Close of the only design shows
# the Start page and the process runs on; Open and New Design start from
# there; the preference turned on in Settings), then
# last-design-close-restart.mitcad-ui in the Docked layout after a restart
# (still on; File › Close of the last design quits MitCad: the run ends
# with "Document windows closed: empty process registry" and the session
# log's end line, before the driver's end).
# Usage: tools/ui-macos-last-design-close-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-last-design-close}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/panel-message-top.json" --save "$UI_OUT/files/part.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
unset MITCAD_SESSION_LOG MITCAD_DEBUG_LOG

# The two runs share the settings (MITCAD_SETTINGS_DIR of ui_init).
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 MITCAD_DEBUG_LOG_DIR=$UI_OUT/logs-close \
  MITCAD_TEST_INPUT=$here/ui-scripts/last-design-close.mitcad-ui \
  ui_run_app last-design-close --ui=qt --no-recovery --no-mcp --chrome=floating --open "$UI_OUT/files/part.mitcad"
ui_expect_log 'TestDriver: done' 'the shared steps ran to their end'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
ui_expect_log '^Last design closed: Start page$' 'File › Close of the last design shows the Start page'
ui_expect_log '^Preferences: quit when the last design is closed on$' 'the preference turned on in Settings'
if grep -q '^Document windows closed: empty process registry$' "$UI_LOG_FILE"; then
  ui_fail 'MitCad quit after the last design closed with the preference off'
else
  ui_pass 'MitCad ran on with no design open'
fi

logs=$UI_OUT/logs-restart
rm -rf "$logs"
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 MITCAD_DEBUG_LOG_DIR=$logs \
  MITCAD_TEST_INPUT=$here/ui-scripts/last-design-close-restart.mitcad-ui \
  ui_run_app last-design-close-restart --ui=qt --no-recovery --no-mcp --chrome=docked \
  --open "$UI_OUT/files/part.mitcad"
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion after a restart'
if grep -q 'TestDriver: done' "$UI_LOG_FILE"; then
  ui_fail 'the driver reached its end: MitCad did not quit'
fi
ui_expect_log '^Last design closed: quitting$' 'the last design closed with the preference on'
ui_expect_log '^Document windows closed: empty process registry$' 'MitCad quit'
session=$(ls "$logs"/session-*.log 2> /dev/null | head -1)
if [ -n "$session" ] && tail -1 "$session" | grep -q '^--- The session ended: '; then
  ui_pass 'the session ended cleanly'
else
  ui_fail "the session log has no clean end: ${session:-none in $logs}"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt closing the last design'
