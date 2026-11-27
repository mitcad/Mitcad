#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Continuity Analysis (mitcad#2521) on macOS, through the test driver: the
# shared steps tools/ui-scripts/continuity-analysis.mitcad-ui (also run by
# tools/ui-continuity-analysis-test.sh and
# tools/ui-windows-continuity-analysis-test.ps1) on the rounded box
# mitcad-cli makes of tools/ui-scripts/continuity-analysis.json. Inspect >
# Continuity Analysis with the tangent rounding's face picked gives its long
# edges G1 and its ends G0; with the body picked in the browser the
# curvature continuous rounding's edges are G2; a curvature tolerance of
# 150 % makes the tangent rounding's G2 too; Escape keeps nothing.
# Usage: tools/ui-macos-continuity-analysis-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-continuity-analysis}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/continuity-analysis.json" --save "$files/continuity-part.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 MITCAD_LOG_PICKS=1 \
  MITCAD_TEST_INPUT=$here/ui-scripts/continuity-analysis.mitcad-ui \
  ui_run_app continuity-analysis --ui=qt --no-recovery --no-mcp --open "$files/continuity-part.mitcad"
ui_expect_log 'Inspect Continuity Analysis: 4 edges: g2 0, g1 2, g0 2, none 0, unmeasured 0' 'the tangent rounding: G1 along, G0 at its ends'
ui_expect_log 'Preview colored edges: ContinuityG1 2, ContinuityG0 2' 'its edges drawn by class'
ui_expect_log 'Command Continuity Analysis cancelled' 'closed, nothing kept'
ui_expect_log 'TestDriver: done' 'Continuity Analysis through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
