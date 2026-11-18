#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build/BuildLists.cpp app/build/ListTables.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/bom_kind.rs core/model/src/document/bom_structure.rs core/model-api/src/cut_sheets.rs core/model-api/src/libraries.rs
# check-all sources: tools/ui-scripts/bom-kinds.mitcad-ui tools/ui-scripts/bom-kinds.json tools/cli
# The Bill of Materials' Kind and the Sheet Metal list (mitcad#1980) with
# the BOM Structure (mitcad#1985) through the real UI, the shared input
# script tools/ui-scripts/bom-kinds.mitcad-ui (also run by
# tools/ui-windows-bom-kinds-test.ps1) on a design that mitcad-cli makes
# (tools/ui-scripts/bom-kinds.json: two sheet-metal components, one placed
# twice, and a plain solid): Kind and BOM Structure as property columns and
# as the window's filters, the Sheet Metal tab with its developed blanks,
# its CSV and PDF, and its parts' Flat Patterns added to a new cut sheet.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-bom-kinds-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-bom-kinds.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/bom-kinds.mitcad-ui
"$cli" run "$here/ui-scripts/bom-kinds.json" --save "$work/bom-kinds.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/bom-kinds.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Bill of Materials kinds driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Build parts list row: Bracket: Sheet metal | Normal' 'Kind and BOM Structure as property columns'
ui_expect_log 'Build parts list filter: kind Sheet metal, structure all' 'the Kind filter'
ui_expect_log 'Build parts list filter: kind all, structure Purchased' 'the BOM Structure filter'
ui_expect_log 'Sheet metal list: 2 row(s)' 'the Sheet Metal tab'
ui_expect_log 'Sheet metal parts added to CutSheet1: 3 part(s)' 'the Flat Patterns on a new cut sheet'
ui_expect_log 'TestDriver: done' 'every step of the Bill of Materials kinds passed'
[ -s "$work/sheet-metal.pdf" ] || ui_fail 'sheet-metal.pdf was not written'
head -c 5 "$work/sheet-metal.pdf" | grep -q '%PDF-' || ui_fail 'sheet-metal.pdf is no PDF'
echo 'ok   sheet-metal.pdf: a PDF'
ui_finish 'UI Bill of Materials kinds'
