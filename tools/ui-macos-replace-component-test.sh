#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Replace Component (mitcad#2640) on macOS, through the test driver: the
# shared steps tools/ui-scripts/replace-component.mitcad-ui (also run by
# tools/ui-replace-component-test.sh and
# tools/ui-windows-replace-component-test.ps1) on the designs mitcad-cli
# makes (replace-component-bracket.json, replace-component-bracket-v2.json,
# and the holder in their folder, replace-component-holder.json). Replace
# Component... from the browser row with Bracket_v2.mitcad shows the
# replacement in all occurrences with the joint kept, OK keeps it as one undo
# step and Undo takes it back; This one from the other row shows it there
# alone and Cancel takes it back.
# Usage: tools/ui-macos-replace-component-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-replace-component}"
UI_TIMEOUT=240
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
{ "$cli" run "$here/ui-scripts/replace-component-bracket.json" --save "$files/Bracket.mitcad" &&
  "$cli" run "$here/ui-scripts/replace-component-bracket-v2.json" --save "$files/Bracket_v2.mitcad" &&
  (cd "$files" && "$cli" run "$here/ui-scripts/replace-component-holder.json" --save "$files/holder.mitcad"); } \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/replace-component.mitcad-ui \
  ui_run_app replace-component --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$files/holder.mitcad"
ui_expect_log 'Replace Component: Bracket replaced by Bracket in 2 occurrences, 0 unmatched' \
  'all occurrences replaced, the joint kept, the name kept'
ui_expect_log 'Replace Component canceled' 'This one previewed and canceled'
ui_expect_log 'TestDriver: done' 'replacing a component'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
