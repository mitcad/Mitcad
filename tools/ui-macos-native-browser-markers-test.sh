#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeMacWindow.mm app/platform/macos/NativeTestInput.mm
# check-all sources: app/MainWindowNative.cpp app/browser/BrowserPanel.cpp app/MainWindowSection.cpp
# check-all sources: tools/ui-scripts/native-macos-browser-markers.mitcad-ui tools/ui-scripts/section-clip-report.json
# What the shared Browser rows show beyond their name, in the AppKit
# outline (mitcad#1352), on the design of tools/ui-scripts/
# section-clip-report.json (Body1 F2.b0, Body2 F4.b0, the Section
# Analyses Section1, shown, and Beyond), with
# MITCAD_TEST_SECTION_CLIP_FAIL=F2.b0 making Body1's clips fail:
#   - Section1's row gets the warning's colour, badge and tooltip when the
#     section worker reports the failed clip after the rows were built,
#     loses them when hidden and gets them again when shown (mitcad#1230);
#     Beyond's row, whose plane cuts no shown body, says so in its tooltip
#     with its plain icon.
#   - Transparent from Body2's context menu gives its row the half-open
#     eye, hidden the closed one, shown again the half-open one, and
#     Transparent again the open eye (mitcad#1093).
# The Make tab's marker is checked by app.ui-macos-native-make-tab.
# Usage: tools/ui-macos-native-browser-markers-test.sh <app> <mitcad-cli> <output>
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$2" run "$here/ui-scripts/section-clip-report.json" --save "$MITCAD_TEST_WORK/markers.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_SECTION_CLIP_FAIL=F2.b0 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-browser-markers.mitcad-ui \
  ui_run_app markers --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/markers.mitcad"
ui_expect_log 'TestDriver: done' 'native Browser row markers workflow reached its end'
ui_expect_no_log 'TestDriver: FAIL|Native Mac canvas:|OCCT view event failed' 'native Browser row markers retain input and rendering'
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo "PASS: native Browser row analysis notes and transparent eyes"
