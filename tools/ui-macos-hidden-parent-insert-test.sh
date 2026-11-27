#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Inserting into a component under a hidden parent (mitcad#1995, GitHub
# mitcad/Mitcad#430) on macOS, through the test driver: the shared steps
# tools/ui-scripts/hidden-parent-insert.mitcad-ui (also run by
# tools/ui-hidden-parent-insert-test.sh and
# tools/ui-windows-hidden-parent-insert-test.ps1) on the designs mitcad-cli
# makes (tools/ui-scripts/hidden-parent-insert.json and
# hidden-parent-insert-part.json). Fairlead:1 hidden, the rows in it have
# the dimmed closed eye; with Post:1 active, an insert and New Component
# show Fairlead:1 in their undo step and say so in the status bar; a click
# on Post:1's dimmed eye shows Fairlead:1.
# Usage: tools/ui-macos-hidden-parent-insert-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-hidden-parent-insert}"
UI_TIMEOUT=240
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
{ "$cli" run "$here/ui-scripts/hidden-parent-insert-part.json" --save "$files/part.mitcad" &&
  "$cli" run "$here/ui-scripts/hidden-parent-insert.json" --save "$files/fairlead.mitcad"; } \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
# The script with the inserted design's path.
sed "s|@PART@|$files/part.mitcad|" "$here/ui-scripts/hidden-parent-insert.mitcad-ui" > "$files/hidden-parent-insert.mitcad-ui"
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$files/hidden-parent-insert.mitcad-ui \
  ui_run_app hidden-parent-insert --ui=qt --no-recovery --no-mcp --open "$files/fairlead.mitcad"
ui_expect_log 'Browser eye Root/Fairlead:1/Post:1: hidden with Fairlead:1' "Post:1's eye is hidden with Fairlead:1"
ui_expect_log 'Hidden parents shown: Fairlead:1 \(by insert_component\)' 'the insert shows Fairlead:1'
ui_expect_log 'Fairlead:1 was hidden; shown so the new component can be seen[.]' 'the status bar says so'
ui_expect_log 'Hidden parents shown: Fairlead:1 \(by create_component\)' 'New Component shows Fairlead:1'
ui_expect_log 'Visibility Root/Fairlead:1/Post:1: shown with its parents \(O1\)' "Post:1's dimmed eye shows Fairlead:1"
ui_expect_log 'TestDriver: done' 'inserting under a hidden parent'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
