#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionPreviewPicks.cpp
# check-all sources: app/OcctViewer.cpp app/framework/TestDriver.cpp app/commands/FilletCommands.cpp
# check-all sources: geometry/src/item_origin.cpp geometry/src/dressup_check.cpp geometry/src/dressup.cpp
# check-all sources: tools/ui-scripts/fillet-preview-pick.mitcad-ui tools/ui-scripts/fillet-preview-pick.json tools/cli
# A pick on Fillet's preview names the edge before the fillet it comes from
# (mitcad#1458, GitHub mitcad/Mitcad#177) through the real UI, the shared
# input script tools/ui-scripts/fillet-preview-pick.mitcad-ui (also run by
# tools/ui-windows-fillet-preview-pick-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/fillet-preview-pick.json): Fillet1,
# rounding a block's two end foot edges on a plate, edited from the
# timeline; the front's foot edge between the roundings' ends, clicked in
# the preview without Ctrl, joins Edges, and the preview of the three
# roundings warns of nothing hanging out.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fillet-preview-pick-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fillet-preview-pick.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-preview-pick.mitcad-ui
"$cli" run "$here/ui-scripts/fillet-preview-pick.json" --save "$work/fillet-preview-pick.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/fillet-preview-pick.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Fillet preview pick driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Editing F3 with Fillet' 'Fillet1 opens for editing with its preview'
ui_expect_log 'Fillet Edges: 3 edges' 'the foot edge the preview cut short joins Edges without Ctrl'
ui_expect_log 'Edited F3' 'OK keeps the three roundings'
ui_expect_log 'TestDriver: done' 'a pick on the preview names the edge before the fillet'
ui_finish 'UI a pick on the preview names the edge before the fillet'
