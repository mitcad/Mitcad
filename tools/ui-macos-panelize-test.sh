#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Panelize (mitcad#2005) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/panelize.mitcad-ui (also run by tools/ui-panelize-test.sh
# and tools/ui-windows-panelize-test.ps1) on the design mitcad-cli makes
# (tools/ui-scripts/panelize.json): a box made a cabinet of 18 mm panels in
# three steps, the last with Remove Remainder.
# Usage: bash tools/ui-macos-panelize-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-panelize}"
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/panelize.mitcad
"$2" run "$here/ui-scripts/panelize.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/panelize.mitcad-ui ui_run_app panelize \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log 'TestDriver: done' 'the cabinet made of five panels in three steps'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Panelize; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: Panelize makes a cabinet of five panels in three steps"
