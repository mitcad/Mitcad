#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/RememberedValues.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sheet-flange-remembered.mitcad-ui tools/ui-scripts/sheet-flange-remembered.json
# Flange's remembered choices (mitcad#1868, GitHub mitcad/Mitcad#348)
# through the real UI, the shared input script
# tools/ui-scripts/sheet-flange-remembered.mitcad-ui (also run by
# tools/ui-windows-sheet-flange-remembered-test.ps1 and
# tools/ui-macos-sheet-flange-remembered-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/sheet-flange-remembered.json): a Flange
# with Measured Inner Length and Position Outside, OK; Flange again shows
# both; the earlier flange edited loads its own Tangent Length and Bend
# Line; an edit's OK keeps nothing.
#
# Runs headless on Xvfb (see ui-test-lib.sh), with the remembered values on
# (the library turns them off) in the run's own settings.
# Usage: tools/ui-sheet-flange-remembered-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-flange-remembered.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-flange-remembered.mitcad-ui
export MITCAD_REMEMBER_PANEL_VALUES=1
design=$work/sheet-flange-remembered.mitcad
"$cli" run "$here/ui-scripts/sheet-flange-remembered.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet flange remembered driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added Flange as F5' 'the flange with Inner Length at Outside was added'
ui_expect_log 'Panel Flange position: remembered outside' 'Flange again started with the last OK'"'"'s Position'
ui_expect_log 'Edited F5' 'the new flange was edited'
ui_expect_log 'TestDriver: done' 'a new Flange starts with the last OK'"'"'s Measured and Position, an edit loads its own'
ui_finish 'UI sheet flange remembered choices'
