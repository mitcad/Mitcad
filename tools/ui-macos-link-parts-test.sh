#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Linked parts of a design (mitcad#2130, GitHub mitcad/Mitcad#479) on
# macOS, through the test driver: the shared steps
# tools/ui-scripts/link-parts.mitcad-ui (also run by
# tools/ui-link-parts-test.sh and tools/ui-windows-link-parts-test.ps1) on
# the designs mitcad-cli makes (tools/ui-scripts/link-parts-source.json and
# link-parts-holder.json). Insert Component > Linked > Choose parts... with
# Bracket:1 ticked links its body alone; Choose Linked Parts... from the
# linked component's browser row adds Bracket:2, one undo step, and Undo
# takes it back. Qt's own file dialog (--no-native-dialogs), so that the
# path can be typed.
# Usage: tools/ui-macos-link-parts-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-link-parts}"
UI_TIMEOUT=240
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
{ "$cli" run "$here/ui-scripts/link-parts-source.json" --save "$files/link-parts-source.mitcad" &&
  "$cli" run "$here/ui-scripts/link-parts-holder.json" --save "$files/holder.mitcad"; } \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
# The script with the linked design's path.
sed "s|@SOURCE@|$files/link-parts-source.mitcad|" "$here/ui-scripts/link-parts.mitcad-ui" > "$files/link-parts.mitcad-ui"
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$files/link-parts.mitcad-ui \
  ui_run_app link-parts --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$files/holder.mitcad"
ui_expect_log 'Insert Component: linked, chosen parts' 'Linked > Choose parts... chosen'
ui_expect_log 'Linked parts of link-parts-source: Bracket:1' 'the insert links Bracket:1 alone'
ui_expect_log 'Choose Parts: O1, O2' 'Choose Linked Parts... ticks Bracket:2 too'
ui_expect_log 'TestDriver: done' 'linking chosen parts of a design'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
