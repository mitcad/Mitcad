#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-title-block-sheets.mitcad-ui tools/ui-scripts/drawing-title-block-sheets.json
# Delete Sheet while the title block is edited (mitcad#1841) through the
# real UI, the shared input script
# tools/ui-scripts/drawing-title-block-sheets.mitcad-ui (also run by
# tools/ui-windows-drawing-title-block-sheets-test.ps1 and
# tools/ui-macos-drawing-title-block-sheets-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/drawing-title-block-sheets.json: a
# drawing of three ISO sheets): the third sheet's block edited and the
# first sheet deleted, Done writes the block onto the sheet edited and no
# other; the sheet edited deleted ends the editor.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-title-block-sheets-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-title-block-sheets.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-title-block-sheets.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-title-block-sheets.json" --save "$work/sheets.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/sheets.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'title block sheets driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Title block editor: on, sheet 3: ' 'the editor on the third sheet'
ui_expect_log 'Title block editor: cancelled' 'the sheet edited deleted ended the editor'
ui_expect_log 'TestDriver: done' 'Done wrote the block onto the sheet edited after another sheet was deleted'
ui_finish 'UI drawing title block with sheets deleted'
