#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/occurrence_order.rs
# check-all sources: core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/browser-reorder.mitcad-ui tools/ui-scripts/browser-reorder-check.json
# The order of components in the Browser (mitcad#2104) on macOS, the same
# shared script as the Linux and Windows tests'
# (tools/ui-scripts/browser-reorder.mitcad-ui) on the design mitcad-cli makes (tools/ui-scripts/browser-reorder.json):
# the third component dragged before the first, Move Down, Undo once, a
# drop onto another component's folder refused; the saved design checked
# with browser-reorder-check.json.
# Usage: tools/ui-macos-browser-reorder-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-browser-reorder}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/browser-reorder.mitcad
"$2" run "$here/ui-scripts/browser-reorder.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/browser-reorder.mitcad-ui \
  ui_run_app browser-reorder --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log 'TestDriver: done' 'browser-reorder: the shared steps'
ui_expect_no_log '^TestDriver: FAIL' 'browser-reorder: no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
if "$2" run "$here/ui-scripts/browser-reorder-check.json" --open "$design" > "$UI_OUT/check.log" 2>&1; then
  ui_pass 'the saved design (browser-reorder-check.json)'
else
  cat "$UI_OUT/check.log" >&2
  ui_fail 'the saved design (browser-reorder-check.json)'
  exit 1
fi
[ "$UI_FAILURES" = 0 ] || exit 1
