#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: core/model/src/sheet core/model/src/document/flat_patterns.rs core/model-api/src/lib.rs
# check-all sources: tools/ui-scripts/sheet-corner-edit.mitcad-ui tools/ui-scripts/sheet-corner-edit.json tools/cli
# A saved Corner Round beside a bend edited from the timeline (mitcad#1766)
# through the real UI, the shared input script
# tools/ui-scripts/sheet-corner-edit.mitcad-ui (also run by
# tools/ui-windows-sheet-corner-edit-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/sheet-corner-edit.json): once the timeline is
# rolled back to the feature, the model's sheet_corner_edges finds its
# corner on the edge between the flange's wall and the bend's side, which
# the panel lists and the view highlights.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-corner-edit-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sheet-corner-edit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/sheet-corner-edit.mitcad-ui
design=$work/sheet-corner-edit.mitcad
"$cli" run "$here/ui-scripts/sheet-corner-edit.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sheet corner edit driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Corner Round corners where the feature finds them: edge E{F6:sheet_bend_side0' 'the corner was found on the edge beside the bend'
ui_expect_log 'TestDriver: done' 'the edited Corner Round lists and highlights its corner beside the bend'
ui_finish 'UI sheet corner edit'
