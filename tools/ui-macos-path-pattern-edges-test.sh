#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Pattern on Path along a body's edges with Tangent Chain (mitcad#2105,
# GitHub mitcad/Mitcad#322) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/path-pattern-edges.mitcad-ui (also run by
# tools/ui-path-pattern-edges-test.sh and
# tools/ui-windows-path-pattern-edges-test.ps1) on the block mitcad-cli
# makes (tools/ui-scripts/path-pattern-edges.json); then mitcad-cli checks
# that the copies follow the rounded outline
# (tools/ui-scripts/path-pattern-edges-check.json).
# Usage: bash tools/ui-macos-path-pattern-edges-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-path-pattern-edges}"
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/path-pattern-edges.mitcad
"$2" run "$here/ui-scripts/path-pattern-edges.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/path-pattern-edges.mitcad-ui ui_run_app path-pattern-edges \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log 'Hover chain: 5 edge[(]s[)] of F2[.]b0' "the edge's hover shows its tangent chain"
ui_expect_log 'Added Pattern on Path' 'the pattern made along the edge'
ui_expect_log 'TestDriver: done' 'Pattern on Path along a body edge with Tangent Chain'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if ! "$2" run "$here/ui-scripts/path-pattern-edges-check.json" --open "$design" > "$UI_OUT/check.log" 2>&1; then
  cat "$UI_OUT/check.log" >&2
  ui_fail 'the copies do not follow the outline'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Pattern on Path along a body edge; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: Pattern on Path along a body edge with Tangent Chain"
