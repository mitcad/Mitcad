#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/resources/layout/default.json
# check-all sources: core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# check-all sources: tools/ui-scripts/sheet-hem.mitcad-ui tools/ui-scripts/sheet-hem.json
# The Hem command (mitcad#2112) through the real UI, the shared input
# script tools/ui-scripts/sheet-hem.mitcad-ui (also run by
# tools/ui-windows-sheet-hem-test.ps1 and tools/ui-macos-sheet-hem-test.sh)
# on a design that mitcad-cli makes (tools/ui-scripts/sheet-hem.json): a
# Flat hem made on a side face, opened in Hem and changed to Open, Rolled
# and Double with their volumes, then unfolded (the flat volume: the
# developed area times the thickness) and refolded.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-hem-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-hem.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-hem.mitcad-ui
design=$work/sheet-hem.mitcad
"$cli" run "$here/ui-scripts/sheet-hem.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet hem driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added Hem as F4' 'the flat hem was added'
ui_expect_log 'Editing F4 with Hem' 'the hem opened in Hem'
ui_expect_log 'TestDriver: done' 'a hem made, changed to Open, Rolled and Double, unfolded and refolded'
ui_finish 'UI sheet hem'
