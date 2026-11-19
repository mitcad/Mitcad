#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/commands/SheetProcessCommands.cpp core/model/src/sheet/process.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sheet-process.mitcad-ui tools/ui-scripts/sheet-process.json
# Sheet process profiles (mitcad#1939) through the real UI: the shared input script
# tools/ui-scripts/sheet-process.mitcad-ui (also run by
# tools/ui-windows-sheet-process-test.ps1 and tools/ui-macos-sheet-process-test.sh) on a
# design that mitcad-cli makes (tools/ui-scripts/sheet-process.json). Board rules with a crease table made in the Sheet Rules panel, Corner Relief greyed for them, a board sheet, the rules edited loading their profile.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-process-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-process.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-process.mitcad-ui
design=$work/sheet-process.mitcad
"$cli" run "$here/ui-scripts/sheet-process.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet-process driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sheet rules: thickness .*, board, crease_table' 'board rules with a crease table were added'
ui_expect_log 'Editing F2 with Sheet Rules' 'the rules were edited'
ui_expect_log 'TestDriver: done' 'Sheet process profiles (mitcad#1939): every step of the script'
ui_finish 'UI sheet-process'
