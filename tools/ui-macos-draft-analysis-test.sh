#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Draft Analysis (mitcad#2166, GitHub mitcad/Mitcad#535) on macOS, through
# the test driver: the shared steps tools/ui-scripts/draft-analysis.mitcad-ui
# (also run by tools/ui-draft-analysis-test.sh and
# tools/ui-windows-draft-analysis-test.ps1) on the drafted part mitcad-cli
# makes of tools/ui-scripts/draft-analysis.json. Inspect > Draft Analysis
# gives the three classes' areas pulled along +Z, then at 3 degrees,
# flipped and pulled along Plane1 picked in the browser; OK keeps it as
# Draft1, which is hidden, shown, edited, saved (checked in the file here),
# deleted and undone.
# Usage: tools/ui-macos-draft-analysis-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-draft-analysis}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/draft-analysis.json" --save "$files/draft-part.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/draft-analysis.mitcad-ui \
  ui_run_app draft-analysis --ui=qt --no-recovery --no-mcp --open "$files/draft-part.mitcad"
ui_expect_log 'Inspect Draft Analysis: pull \(0, 0, 1\), angle 1: Body1 along 1249\.4 mm\^2' 'pulled along +Z at 1 degree'
ui_expect_log 'Kept Draft Analysis as Draft1' 'kept in the document'
ui_expect_log 'Deleted analysis Draft1' 'deleted from its context menu'
ui_expect_log 'TestDriver: done' 'Draft Analysis through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
if grep -q '"type": "draft"' "$files/draft-part.mitcad" && grep -q '"name": "Draft1"' "$files/draft-part.mitcad"; then
  echo "ok   the analysis is in the saved file"
else
  ui_fail "no Draft1 draft analysis in the saved file"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
