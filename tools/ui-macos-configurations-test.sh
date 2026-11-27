#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/Configuration app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli core/model-api/src/configurations.rs
# Configurations (mitcad#807) on macOS, through the test driver
# (tools/ui-scripts/configurations.mitcad-ui, the same script as the
# Windows test's): the Configurations window opens with its options and
# row, and a choice in the browser's Configuration bar changes the body;
# the window's minimum height fits a screen 720 logical px high
# (mitcad#2036).
# Usage: tools/ui-macos-configurations-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-configurations}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/configurations.json" --save "$UI_OUT/files/configurations.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/configurations.mitcad-ui \
  ui_run_app configurations --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/configurations.mitcad"
ui_expect_log 'TestDriver: done' 'the window opened and the bar'"'"'s choice changed the body'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
# The window's minimum height on every tab fits a screen 720 logical px
# high with a typical window frame (48 px; mitcad#2036): the Rules
# palette's full height is its preferred height, not its minimum.
minimum=$(sed -n 's/.*Configurations window minimum size [0-9]*x\([0-9]*\).*/\1/p' "$UI_LOG_FILE" | tail -1)
if [ -z "$minimum" ]; then
  ui_fail "the window's minimum size is not in the log"
elif [ $((minimum + 48)) -gt 720 ]; then
  ui_fail "the window's minimum height $minimum px does not fit 720 px with its frame"
else
  ui_pass "the window's minimum height $minimum px fits 720 px with its frame"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
