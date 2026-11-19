#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# check-all sources: tools/ui-scripts/sheet-convert.mitcad-ui tools/ui-scripts/sheet-convert.json
# Convert to Sheet Metal (mitcad#1949, GitHub mitcad/Mitcad#377) through
# the real UI, the shared input script tools/ui-scripts/sheet-convert.mitcad-ui
# (also run by tools/ui-windows-sheet-convert-test.ps1 and
# tools/ui-macos-sheet-convert-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/sheet-convert.json): a solid box's bottom face and its
# four bottom edges picked, OK, the tray one body of the volume
# cli.sheet_convert_tray gives, its corners ripped; the conversion edited
# loads its picks.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-convert-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-convert.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-convert.mitcad-ui
design=$work/sheet-convert.mitcad
"$cli" run "$here/ui-scripts/sheet-convert.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet convert driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Convert to Sheet Metal: ok' 'the box with its base and four bend edges previewed as a sheet'
ui_expect_log 'Added Convert to Sheet Metal as F3' 'the conversion was added'
ui_expect_log 'Editing F3 with Convert to Sheet Metal' 'the conversion was edited'
ui_expect_log 'TestDriver: done' 'a solid tray converted through the panel, its corners ripped'
ui_finish 'UI sheet convert'
