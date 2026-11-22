#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/selection_sets.rs
# check-all sources: core/model-api/src/selection_sets.rs tools/cli
# check-all sources: tools/ui-scripts/selection-sets.mitcad-ui tools/ui-scripts/selection-sets-check.json
# Selection sets (mitcad#2192) on macOS, the same shared script as the
# Linux and Windows tests' (tools/ui-scripts/selection-sets.mitcad-ui) on
# the design mitcad-cli makes (tools/ui-scripts/selection-sets.json): a
# body, a used sketch and a feature into a new set, renamed in place; the
# set's row selects them; Find in Timeline and Find in Browser from member
# rows; Remove from Set, Delete and Undo; the saved design checked with
# selection-sets-check.json.
# Usage: tools/ui-macos-selection-sets-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-selection-sets}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/selection-sets.mitcad
"$2" run "$here/ui-scripts/selection-sets.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/selection-sets.mitcad-ui \
  ui_run_app selection-sets --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log 'TestDriver: done' 'selection-sets: the shared steps'
ui_expect_no_log '^TestDriver: FAIL' 'selection-sets: no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
if "$2" run "$here/ui-scripts/selection-sets-check.json" --open "$design" > "$UI_OUT/check.log" 2>&1; then
  ui_pass 'the saved design (selection-sets-check.json)'
else
  cat "$UI_OUT/check.log" >&2
  ui_fail 'the saved design (selection-sets-check.json)'
  exit 1
fi
[ "$UI_FAILURES" = 0 ] || exit 1
