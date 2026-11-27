#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserBackground.cpp app/browser/BrowserPanel.cpp app/framework/GlassCard.cpp
# check-all sources: app/framework/FloatingLayout.cpp app/MainWindow.cpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/browser-widths.mitcad-ui
# check-all sources: tools/ui-scripts/browser-widths-restart.mitcad-ui tools/ui-scripts/browser-widths.json
# The Browser's width in the Floating layout (mitcad#1932) on macOS, through
# the test driver (tools/ui-scripts/browser-widths.mitcad-ui and
# browser-widths-restart.mitcad-ui, the same scripts as the Linux and
# Windows tests'): Rows at the start without a saved choice (mitcad#2144);
# with Rows a long component name widens the rows and the
# card, a longer one stops at a third of the view and is elided, a limit
# changed in Settings takes effect, the card shrinks back and a click beside
# the rows reaches the view; with Card the dragged width grip widens the
# card, and the width comes back after a restart.
# Usage: tools/ui-macos-browser-widths-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-browser-widths}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/browser-widths.json" --save "$UI_OUT/files/browser-widths.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
# The two runs share the settings (MITCAD_SETTINGS_DIR of ui_init).
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/browser-widths.mitcad-ui \
  ui_run_app browser-widths --ui=qt --no-recovery --no-mcp --chrome=floating --open "$UI_OUT/files/browser-widths.mitcad"
ui_expect_log 'Browser card width 380 remembered' 'the dragged Card width is remembered'
ui_expect_log 'TestDriver: done' 'the browser widths'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/browser-widths-restart.mitcad-ui \
  ui_run_app browser-widths-restart --ui=qt --no-recovery --no-mcp --chrome=floating \
  --open "$UI_OUT/files/browser-widths.mitcad"
ui_expect_log 'Browser card width 380 [(]remembered[)]' 'the Card width comes back after a restart'
ui_expect_log 'TestDriver: done' 'the browser widths after a restart'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion after a restart'
[ "$UI_FAILURES" = 0 ] || exit 1
