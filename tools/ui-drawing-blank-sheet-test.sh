#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing_templates.rs core/model-drawing/src core/model/src/drawing
# check-all sources: tools/ui-scripts/drawing-blank-sheet.mitcad-ui tools/ui-scripts/drawing-blank-sheet.json
# Blank chosen in Sheet… (mitcad#2325) through the real UI, the shared
# input script tools/ui-scripts/drawing-blank-sheet.mitcad-ui (also run by
# tools/ui-windows-drawing-blank-sheet-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/drawing-blank-sheet.json: an ISO
# drawing with a view): Blank hides the title block's fields and the sheet
# then shows no frame and no title block; ISO chosen again brings them
# back with the title kept.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-blank-sheet-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-blank-sheet.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-blank-sheet.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-blank-sheet.json" --save "$work/blank.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/blank.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'blank sheet driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Drawing sheet dialog title block fields: hidden' 'Blank hid the title block fields'
ui_expect_log 'Drawing sheet border: frame no, title block no' 'the Blank sheet shows no frame and no title block'
ui_expect_log 'TestDriver: done' 'ISO brought the frame and the block back with the title kept'
ui_finish 'UI drawing Blank sheet'
