#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/PickContext.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp tools/cli tools/ui-scripts/region-pick-inside-body.mitcad-ui
# A sketch region inside a body is picked before the body's face in front
# of it (mitcad#1222) through the real UI: the shared input script
# tools/ui-scripts/region-pick-inside-body.mitcad-ui (also run by
# tools/ui-windows-region-pick-inside-body-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/region-pick-inside-body.json): a block
# with a sketch on a plane through its middle and a cut Extrude of its
# square. Editing the Extrude seen from the top, a click on the square
# picks the region (logged), not the block's top face; a click on the top
# face where no region is under the cursor still picks the face.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-region-pick-inside-body-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-region-pick-inside-body.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/region-pick-inside-body.mitcad-ui
"$cli" run "$here/ui-scripts/region-pick-inside-body.json" --save "$work/region-inside.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/region-inside.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'region pick driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the region inside the block picked before its face, the face where no region is'
ui_finish 'UI region pick inside a body'
