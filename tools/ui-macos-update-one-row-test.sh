#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/update app/MainWindow.cpp app/WindowManager.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/update-one-row.mitcad-ui tools/ui-scripts/panel-message-top.json tools/cli
# One update for the whole application (mitcad#1890) on macOS, through the
# test driver (tools/ui-scripts/update-one-row.mitcad-ui, the same script
# as the Linux and Windows tests') with the update tests' fake offer (the
# driver's step update-fake-offer: no server, no request), on two cubes
# that mitcad-cli makes: Install accepted in the first design's window, a
# second design opened shows the download in its row at once, the first
# design closed leaves the row in the second, where the download goes on
# and reaches "downloaded"; then Exit during a download asks, and Keep
# Downloading keeps MitCad open until it is downloaded.
# Usage: tools/ui-macos-update-one-row-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-update-one-row}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
for name in first second; do
  "$2" run "$here/ui-scripts/panel-message-top.json" --save "$UI_OUT/files/$name.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
    cat "$UI_OUT/setup.log" >&2
    exit 1
  }
done

# MITCAD_NO_UPDATE_CHECK stays set (ui_init): no automatic check and no
# request; the fake offer answers Check for Updates all the same.
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/update-one-row.mitcad-ui \
  ui_run_app update-one-row --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/first.mitcad"
ui_expect_log 'TestDriver: done' "the shared steps ran to their end"
ui_expect_no_log 'TestDriver: FAIL' "no failed driver assertion"
ui_expect_log '^Update row: in 2 window\(s\)$' "the second design's window shows the update's row"
# After the first window closed: the download went on in the second.
after=$(sed -n '/^Update row: in 2 window(s)$/,$p' "$UI_LOG_FILE" | sed -n '/^Update row: in 1 window(s)$/,$p')
if grep -qE '^Update download: [0-9]+%$' <<< "$after"; then
  ui_pass "the download went on after the first window closed"
else
  ui_fail "no progress of the download after the first window closed"
fi
if grep -qF 'Update notice (message): MitCad 999.0.0 is downloaded.' <<< "$after"; then
  ui_pass "the second window's row reached downloaded"
else
  ui_fail "the second window's row did not reach downloaded"
fi
if grep -qE '^Update: cancelled' "$UI_LOG_FILE"; then ui_fail "the download was cancelled"; fi
ui_expect_log '^Update question: MitCad 999\.0\.0 is still downloading$' "Exit during the download asks"
ui_expect_log '^Update: the download goes on, MitCad stays open$' "Keep Downloading keeps MitCad open"
[ "$UI_FAILURES" = 0 ] || exit 1
