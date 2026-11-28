#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/report app/framework/TestDriver.cpp app/platform/MacChrome.hpp tools/cli
# check-all sources: tools/ui-scripts/feedback-region.mitcad-ui tools/ui-scripts/feedback-region.json
# Send Feedback on a Region (mitcad#2616) in the Qt frontend on macOS: the
# shared steps tools/ui-scripts/feedback-region.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/feedback-region.json). F9, a drag
# over the view and a browser row on the overlay, the form with Bug and the
# region's screenshot, private; Esc and a click cancel; F9 held released
# after the drag takes the region. Without the Screen Recording permission
# (never asked in a test's run) the picture is the window's, cropped to the
# region, which the log says. As tools/ui-feedback-region-test.sh (Linux)
# and tools/ui-windows-feedback-region-test.ps1 (Windows).
# Usage: bash tools/ui-macos-feedback-region-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-feedback-region}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/feedback-region.json" --save "$MITCAD_TEST_WORK/region.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/feedback-region.mitcad-ui \
  ui_run_app feedback-region --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/region.mitcad"
ui_expect_log 'Feedback form for a region: kind bug, screenshot on' 'the form for a region'
ui_expect_log 'Feedback region: canceled [(]Esc[)]' 'Esc cancels'
ui_expect_log 'TestDriver: done' 'Send Feedback on a Region'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: feedback region; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: feedback region; evidence in $UI_OUT"
