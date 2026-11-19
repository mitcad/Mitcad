#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindowMeasurements.cpp app/MainWindow.hpp app/view/MeasurementOverlay.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/document/display.rs core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/annotations-folder-eye.json tools/ui-scripts/annotations-folder-eye-check.json
# check-all sources: tools/ui-scripts/annotations-folder-eye.mitcad-ui tools/ui-scripts/annotations-folder-eye-reopen.mitcad-ui
# The Browser's Annotations folder's eye (mitcad#2041) through the real UI,
# the shared input scripts tools/ui-scripts/annotations-folder-eye.mitcad-ui
# and annotations-folder-eye-reopen.mitcad-ui (also run by
# tools/ui-windows-annotations-folder-eye-test.ps1 and
# tools/ui-macos-annotations-folder-eye-test.sh) on a design mitcad-cli
# makes (tools/ui-scripts/annotations-folder-eye.json): a block with three
# offsets, Offset2 hidden by its own light bulb.
#   - The folder's eye hides every annotation (none drawn, none picked
#     where one was, the row greyed), one undo step `Hide Annotations`;
#     shown again, the same two are drawn.
#   - Saved hidden: the file keeps the folder hidden and the annotations'
#     own light bulbs as they were (annotations-folder-eye-check.json);
#     opened again, none is drawn until the eye shows them; saved shown,
#     nothing of the folder is left in the file.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-annotations-folder-eye-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-annotations-folder-eye.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
design=$work/annotations-folder-eye.mitcad
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/annotations-folder-eye.json" --save "$design" > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli'
}
ui_start_display

# run_phase <script>: one of the shared scripts on the design.
run_phase() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp --open "$design"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$1: the shared steps"
}

run_phase annotations-folder-eye
"$cli" run "$here/ui-scripts/annotations-folder-eye-check.json" --open "$design" > "$work/check.log" 2>&1 || {
  cat "$work/check.log"
  ui_fail 'the saved design does not keep the Annotations folder hidden'
}
echo "ok   saved with the Annotations folder hidden, the annotations' own light bulbs as they were"
run_phase annotations-folder-eye-reopen
actual=$(jq -c '[.display.folders, [.annotations[] | select(.visible == false) | .name]]' "$design") ||
  ui_fail "cannot read $design"
[ "$actual" = '[null,["Offset2"]]' ] || ui_fail "saved shown: the folders and hidden annotations are $actual"
echo "ok   saved shown: nothing of the folder left, Offset2 still hidden"
ui_finish 'UI Annotations folder eye'
