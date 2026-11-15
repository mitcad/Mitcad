#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MoveOccurrenceDialog.cpp app/browser/BrowserController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/move-occurrence-group.mitcad-ui
# Move to Component on a rigid group joined to a shaft (mitcad#1847) on
# macOS, through the test driver (tools/ui-scripts/move-occurrence-group.mitcad-ui,
# the same script as the Linux and Windows tests'): every parent two
# selected occurrences cannot go into says why under it; one of them
# offers the rest of its rigid group; with the shaft too, all three go
# into the empty component in one step.
# Usage: tools/ui-macos-move-occurrence-group-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-move-occurrence-group}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
export MITCAD_TEST_WORK=$UI_OUT/files
"$2" run "$here/ui-scripts/move-occurrence-group.json" --save "$UI_OUT/files/move-occurrence-group.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/move-occurrence-group.mitcad-ui \
  ui_run_app move-occurrence-group --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/move-occurrence-group.mitcad"
ui_expect_log 'Move Component reason shown under Root/Holder:1: Joint1: it joins Bearing:1 to Shaft:1' \
  'the reason under the empty component, in words'
ui_expect_log 'Move Component rigid group offered: Tube:1' 'the rest of the rigid group offered'
ui_expect_log 'TestDriver: done' 'Move to Component on a rigid group'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
