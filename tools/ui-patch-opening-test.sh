#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/framework/OpeningEdges.cpp app/framework/Selection.cpp
# check-all sources: app/OcctViewer.cpp app/PickContext.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandSessionPreviewPicks.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/patch.rs geometry/src/surfaces.cpp tools/cli
# check-all sources: tools/ui-scripts/patch-opening.mitcad-ui tools/ui-scripts/patch-opening.json
# Patch picks a surface body's free edge as its opening and closes it
# (mitcad#1927, GitHub mitcad/Mitcad#379) through the real UI, the shared
# input script tools/ui-scripts/patch-opening.mitcad-ui (also run by
# tools/ui-windows-patch-opening-test.ps1 and
# tools/ui-macos-patch-opening-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/patch-opening.json: a box whose top face was
# unstitched and deleted): the opening's free edges are offered as edges,
# a click on one takes the whole opening, and OK with Stitch makes the box
# a closed solid again.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-patch-opening-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-patch-opening.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/patch-opening.mitcad-ui
"$cli" run "$here/ui-scripts/patch-opening.json" --save "$work/patch-opening.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/patch-opening.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Patch opening driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Pick edge F1.b0/{"faces":' 'a free edge of the opening offered as an edge'
ui_expect_log 'Patch Profiles or Edges: 1 edge' 'a click on it takes the whole opening'
ui_expect_log 'Added patch of 1 items, stitched' 'OK patches the opening and stitches it on'
ui_expect_log 'TestDriver: done' 'the box is a closed solid again'
ui_finish 'UI Patch picks an opening by its free edge and closes it'
