#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build/BuildLists.cpp app/build/ListTables.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/build-lists-text.mitcad-ui tools/ui-scripts/build-lists-text.json tools/cli
# The BUILD lists as text (mitcad#1641) through the real UI, the shared
# input script tools/ui-scripts/build-lists-text.mitcad-ui (also run by
# tools/ui-windows-build-lists-text-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/build-lists-text.json: three components, one with
# a long name): the Cut List's Copy as CSV puts rows ending in CRLF on the
# clipboard and Save as CSV… writes the same; Copy as Text puts the tab's
# tables there as aligned plain text with CRLF line ends, and Save as
# Text… writes exactly that text; the Bill of Materials likewise.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-lists-text-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-build-lists-text.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/build-lists-text.mitcad-ui
"$cli" run "$here/ui-scripts/build-lists-text.json" --save "$work/build-lists-text.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/build-lists-text.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'BUILD lists as text driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Build list copied as CSV: ' 'Copy as CSV copied the cut list'
ui_expect_log 'Build list saved as CSV: ' 'Save as CSV wrote the cut list'
ui_expect_log 'Build list copied as text: ' 'Copy as Text copied the tables'
ui_expect_log 'Build list saved as text: ' 'Save as Text wrote the tables'
ui_expect_log 'TestDriver: done' 'the clipboard and the saved files hold the same text, every line ending in CRLF'
# The saved files outside the application: CRLF line ends only, UTF-8.
for file in cut-list.csv cut-list.txt bill-of-materials.txt; do
  [ -s "$work/$file" ] || ui_fail "$file was not written"
  lines=$(wc -l < "$work/$file")
  crlf=$(LC_ALL=C grep -c $'\r$' "$work/$file")
  last=$(tail -c 2 "$work/$file" | od -An -tx1 | tr -d ' \n')
  [ "$lines" -gt 0 ] && [ "$crlf" = "$lines" ] && [ "$last" = 0d0a ] ||
    ui_fail "$file: $crlf of $lines line(s) end in CRLF, its last bytes $last"
  echo "ok   $file: every line ends in CRLF"
done
grep -q $'^       1  A component with a rather long name\r$' "$work/cut-list.txt" ||
  ui_fail 'cut-list.txt has no aligned row of the component with the long name'
echo 'ok   cut-list.txt: the long name in its padded column'
ui_finish 'UI BUILD lists as text'
