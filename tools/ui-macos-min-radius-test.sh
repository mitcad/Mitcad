#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Minimum Radius (mitcad#2518) on macOS, through the test driver: the shared
# steps tools/ui-scripts/min-radius.mitcad-ui (also run by
# tools/ui-min-radius-test.sh and tools/ui-windows-min-radius-test.ps1) on
# the rounded block mitcad-cli makes of tools/ui-scripts/min-radius.json.
# Inspect > Minimum Radius marks the smallest radius, 2 mm; a Radius Limit of
# 11 mm takes all three roundings, 4 mm the 2 mm one alone; OK keeps it as
# MinRadius1, which is hidden, shown, edited, saved (checked in the file
# here), deleted and undone.
# Usage: tools/ui-macos-min-radius-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-min-radius}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/min-radius.json" --save "$files/min-radius-part.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/min-radius.mitcad-ui \
  ui_run_app min-radius --ui=qt --no-recovery --no-mcp --open "$files/min-radius-part.mitcad"
ui_expect_log 'Inspect Minimum Radius: smallest 2\.000 mm on Body1' 'the smallest radius, 2 mm'
ui_expect_log 'under 4 mm: 1 face\(s\), 62\.8 mm\^2' 'a limit of 4 mm takes the 2 mm rounding alone'
ui_expect_log 'Kept Minimum Radius as MinRadius1' 'kept in the document'
ui_expect_log 'Deleted analysis MinRadius1' 'deleted from its context menu'
ui_expect_log 'TestDriver: done' 'Minimum Radius through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
if grep -q '"type": "min_radius"' "$files/min-radius-part.mitcad" && grep -q '"name": "MinRadius1"' "$files/min-radius-part.mitcad"; then
  echo "ok   the analysis is in the saved file"
else
  ui_fail "no MinRadius1 minimum radius analysis in the saved file"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
