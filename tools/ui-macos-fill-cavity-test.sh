#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Fill Cavity (mitcad#1818) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/fill-cavity.mitcad-ui (also run by tools/ui-fill-cavity-test.sh
# and tools/ui-windows-fill-cavity-test.ps1) on the design mitcad-cli makes
# (tools/ui-scripts/fill-cavity.json): a blind slit filled up to a plane,
# its volume added, and an open groove as the seed refused in words.
# Usage: bash tools/ui-macos-fill-cavity-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-fill-cavity}"
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/fill-cavity.mitcad
"$2" run "$here/ui-scripts/fill-cavity.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/fill-cavity.mitcad-ui ui_run_app fill-cavity \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log 'TestDriver: done' 'the slit filled up to the plane, the open groove refused'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Fill Cavity; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: Fill Cavity fills the slit and refuses the open groove"
