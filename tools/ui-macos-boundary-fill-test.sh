#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Boundary Fill (mitcad#2149) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/boundary-fill.mitcad-ui (also run by
# tools/ui-boundary-fill-test.sh and tools/ui-windows-boundary-fill-test.ps1)
# on the design mitcad-cli makes (tools/ui-scripts/boundary-fill.json): four
# surfaces and two planes filled as a new body, a clicked cell cut from a
# block, Undo, and the first fill edited with its cell.
# Usage: bash tools/ui-macos-boundary-fill-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-boundary-fill}"
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/boundary-fill.mitcad
"$2" run "$here/ui-scripts/boundary-fill.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/boundary-fill.mitcad-ui ui_run_app boundary-fill \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log 'TestDriver: done' 'the closed region filled, a clicked cell cut, the fill edited with its cell'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Boundary Fill; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: Boundary Fill fills the closed region, cuts a clicked cell and edits with its cells"
