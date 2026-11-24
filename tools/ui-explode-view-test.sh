#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowExplode.cpp app/motion/ExplodeViewDialog.cpp app/commands/ExplodeCommands.cpp
# check-all sources: app/view/ExplodeTrailOverlay.cpp app/browser app/drawings app/framework/TestDriver.cpp
# check-all sources: core/model/src/explode.rs core/model/src/document/explode.rs core/model-api/src/explode.rs
# check-all sources: core/model-api/src/drawings.rs tools/cli
# check-all sources: tools/ui-scripts/explode-view.json tools/ui-scripts/explode-view-check.json
# check-all sources: tools/ui-scripts/explode-view.mitcad-ui
# check-all sources: app/commands/InspectCommands.cpp app/framework/CommandSession.cpp core/model-api/src/analysis.rs
# Exploded views (mitcad#2113) through the real UI, the shared input script
# tools/ui-scripts/explode-view.mitcad-ui (also run by
# tools/ui-windows-explode-view-test.ps1 and
# tools/ui-macos-explode-view-test.sh) on a design mitcad-cli makes
# (tools/ui-scripts/explode-view.json): three stacked components and a
# drawing.
#   - Explode's panel: Auto Explode makes Explode1 and shows it exploded
#     with its trail lines; a step added by dragging Explode Step's
#     Distance arrow; the steps played back to assembled and to the end;
#     the Browser's row shows it assembled and exploded; a drawing's base
#     view placed with it, its trail lines thin phantom lines.
#   - Following the design (mitcad#2328): Measure keeps the view exploded
#     and measures two exploded bodies' faces where they are shown; a step
#     along an edge picked as Direction's Picked Geometry, kept as the
#     edge's reference.
#   - Saved: the design keeps the view and the drawing view's explode
#     (explode-view-check.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-explode-view-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-explode-view.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
design=$work/explode-view.mitcad
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/explode-view.json" --save "$design" > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli'
}
ui_start_display
export MITCAD_TEST_INPUT=$here/ui-scripts/explode-view.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'explode-view: driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' 'explode-view: the shared steps'
"$cli" run "$here/ui-scripts/explode-view-check.json" --open "$design" > "$work/check.log" 2>&1 || {
  cat "$work/check.log"
  ui_fail 'the saved design does not keep the explode view'
}
echo "ok   saved with the explode view Explode1 and the drawing view that shows it"
ui_finish 'UI Exploded views'
