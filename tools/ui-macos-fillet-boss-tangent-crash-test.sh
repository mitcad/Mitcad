#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Fillet on the slot-and-boss part (mitcad#1429, GitHub mitcad/Mitcad#173)
# in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/fillet-slot-boss-1429.mitcad-ui (also run by
# tools/ui-windows-fillet-boss-tangent-crash-test.ps1 and
# tools/ui-fillet-boss-tangent-crash-test.sh) on the part mitcad-cli makes
# (tools/ui-scripts/fillet-slot-boss-1429.json): Fillet on the plate's
# top edge that runs tangentially into the boss, its previews at several
# radii, Tangent Chain off and on, hovers, Enter and Escape. The application
# stays up whatever the fillet gives.
# Usage: bash tools/ui-macos-fillet-boss-tangent-crash-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-fillet-boss-tangent-crash}"
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/fillet-boss-tangent-crash.mitcad
"$2" run "$here/ui-scripts/fillet-slot-boss-1429.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/fillet-slot-boss-1429.mitcad-ui ui_run_app fillet-boss-tangent-crash \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log 'TestDriver: done' 'the fillet on the slot-and-boss part, its previews, hovers, Enter and Escape'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: fillet on the slot-and-boss part; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: the fillet on the slot-and-boss part leaves the application up"
