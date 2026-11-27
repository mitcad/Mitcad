#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# check-all sources: tools/ui-scripts/sheet-lofted-flange.mitcad-ui tools/ui-scripts/sheet-lofted-flange.json
# Lofted Flange (mitcad#2512) through the real UI, the shared input script
# tools/ui-scripts/sheet-lofted-flange.mitcad-ui (also run by
# tools/ui-windows-sheet-lofted-flange-test.ps1 and
# tools/ui-macos-sheet-lofted-flange-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/sheet-lofted-flange.json): a rectangle and a
# circle 70 mm above it picked as the profiles, OK, the press brake
# transition one body of the volume cli.sheet_lofted_flange gives; the
# lofted flange edited loads its profiles.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-lofted-flange-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-lofted-flange.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-lofted-flange.mitcad-ui
design=$work/sheet-lofted-flange.mitcad
"$cli" run "$here/ui-scripts/sheet-lofted-flange.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet lofted flange driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Lofted Flange: ok' 'the rectangle and the circle previewed as a lofted flange'
ui_expect_log 'Added Lofted Flange as F5' 'the lofted flange was added'
ui_expect_log 'Editing F5 with Lofted Flange' 'the lofted flange was edited'
ui_expect_log 'TestDriver: done' 'a press brake transition made through the panel'
ui_finish 'UI sheet lofted flange'
