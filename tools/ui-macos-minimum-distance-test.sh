#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Minimum Distance (mitcad#2519) on macOS, through the test driver: the
# shared steps tools/ui-scripts/minimum-distance.mitcad-ui (also run by
# tools/ui-minimum-distance-test.sh and
# tools/ui-windows-minimum-distance-test.ps1) on the two blocks mitcad-cli
# makes of tools/ui-scripts/minimum-distance.json. Inspect > Minimum
# Distance between Body1 and Body2 gives 5 mm, closer than a 6 mm clearance
# and clear of a 4 mm one; OK keeps it as MinDistance1, measured again when
# the blocks' parameter changes, hidden, shown, edited, saved (checked in
# the file here), deleted and undone.
# Usage: tools/ui-macos-minimum-distance-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-minimum-distance}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/minimum-distance.json" --save "$files/distance-blocks.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/minimum-distance.mitcad-ui \
  ui_run_app minimum-distance --ui=qt --no-recovery --no-mcp --open "$files/distance-blocks.mitcad"
ui_expect_log 'Inspect Minimum Distance: 5\.000 mm from Body1' 'Body1 to Body2: 5 mm'
ui_expect_log 'clearance 6 mm: closer' 'closer than a 6 mm clearance'
ui_expect_log 'Kept Minimum Distance as MinDistance1' 'kept in the document'
ui_expect_log 'Minimum distance MinDistance1: 3\.000 mm' 'measured again after the parameter changed'
ui_expect_log 'Deleted analysis MinDistance1' 'deleted from its context menu'
ui_expect_log 'TestDriver: done' 'Minimum Distance through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
if grep -q '"type": "minimum_distance"' "$files/distance-blocks.mitcad" &&
  grep -q '"name": "MinDistance1"' "$files/distance-blocks.mitcad"; then
  echo "ok   the analysis is in the saved file"
else
  ui_fail "no MinDistance1 minimum distance analysis in the saved file"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
