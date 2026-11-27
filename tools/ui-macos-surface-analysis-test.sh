#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Surface quality analyses (mitcad#2517) on macOS, through the test driver:
# the shared steps tools/ui-scripts/surface-analysis.mitcad-ui (also run by
# tools/ui-surface-analysis-test.sh and
# tools/ui-windows-surface-analysis-test.ps1) on the cylinder mitcad-cli
# makes of tools/ui-scripts/surface-analysis.json. Zebra, Curvature Map,
# Isocurve Analysis and Environment Map computed, drawn and kept, Zebra1
# shown again and edited, saved (checked in the file here).
# Usage: tools/ui-macos-surface-analysis-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-surface-analysis}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/surface-analysis.json" --save "$files/surface-part.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/surface-analysis.mitcad-ui \
  ui_run_app surface-analysis --ui=qt --no-recovery --no-mcp --open "$files/surface-part.mitcad"
ui_expect_log 'Inspect Zebra: direction \(0, 0, 1\), 3 stripes, width 0\.3: Body1 dark' 'the zebra stripes'
ui_expect_log 'Kept Zebra as Zebra1' 'the zebra kept'
ui_expect_log 'Inspect Curvature Map: maximum 1/mm, limit 0\.1 \(automatic\): Body1 min 0, max 0\.1' 'the side 0.1/mm'
ui_expect_log 'Inspect Isocurve Analysis: 2 per face, curvature colored: Body1 12 curves' 'the iso curves'
ui_expect_log 'Inspect Environment Map: studio picture: Body1' 'the environment reflected'
ui_expect_log 'Kept Environment Map as Environment1' 'the environment map kept'
ui_expect_log 'TestDriver: done' 'the surface quality analyses through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
missing=""
for type in zebra curvature isocurve environment; do
  grep -q "\"type\": \"$type\"" "$files/surface-part.mitcad" || missing="$missing $type"
done
if [ -z "$missing" ]; then
  echo "ok   the four analyses are in the saved file"
else
  ui_fail "not in the saved file:$missing"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
