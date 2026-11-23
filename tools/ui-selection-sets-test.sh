#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/selection_sets.rs
# check-all sources: core/model-api/src/selection_sets.rs tools/cli
# check-all sources: tools/ui-scripts/selection-sets.json tools/ui-scripts/selection-sets-check.json
# check-all sources: tools/ui-scripts/selection-sets.mitcad-ui
# Selection sets (mitcad#2192) through the real UI, the shared input script
# tools/ui-scripts/selection-sets.mitcad-ui (also run by
# tools/ui-windows-selection-sets-test.ps1 and
# tools/ui-macos-selection-sets-test.sh) on a design mitcad-cli makes
# (tools/ui-scripts/selection-sets.json): Sketch1 used by Extrude1, and the
# box Block.
#   - A body, a used sketch and a feature selected, Create Selection Set,
#     renamed Favourites in place; a click on the set selects all three;
#     Find in Timeline on the feature's member row and Find in Browser on
#     the sketch's reveal the originals; Remove from Set; the Delete key on
#     a member row removes the link and leaves the body (mitcad#2219);
#     Delete the set; Undo of each step.
#   - Saved: the design keeps the set (selection-sets-check.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-selection-sets-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-selection-sets.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
design=$work/selection-sets.mitcad
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/selection-sets.json" --save "$design" > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli'
}
ui_start_display
export MITCAD_TEST_INPUT=$here/ui-scripts/selection-sets.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'selection-sets: driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' 'selection-sets: the shared steps'
"$cli" run "$here/ui-scripts/selection-sets-check.json" --open "$design" > "$work/check.log" 2>&1 || {
  cat "$work/check.log"
  ui_fail 'the saved design does not keep the selection set'
}
echo "ok   saved with the selection set Favourites: Block, Sketch1 and Extrude1"
ui_finish 'UI Selection sets'
