#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/occurrence_folders.rs
# check-all sources: core/model/src/document/bodies.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/browser-folders.json tools/ui-scripts/browser-folders-check.json
# check-all sources: tools/ui-scripts/browser-folders.mitcad-ui
# Folders in the Browser (mitcad#2103) through the real UI, the shared
# input script tools/ui-scripts/browser-folders.mitcad-ui (also run by
# tools/ui-windows-browser-folders-test.ps1 and
# tools/ui-macos-browser-folders-test.sh) on a design mitcad-cli makes
# (tools/ui-scripts/browser-folders.json): three components and two bodies.
#   - Two components into a New Folder from the row menu, renamed in place,
#     closed; its eye hides both; Rename and Ungroup; a component dragged
#     onto the folder's row and a drop refused; two bodies into a folder
#     and dragged out of it; Undo of each step.
#   - Saved: the design keeps the components' folder
#     (browser-folders-check.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-folders-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-folders.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
design=$work/browser-folders.mitcad
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/browser-folders.json" --save "$design" > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli'
}
ui_start_display
export MITCAD_TEST_INPUT=$here/ui-scripts/browser-folders.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'browser-folders: driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' 'browser-folders: the shared steps'
"$cli" run "$here/ui-scripts/browser-folders-check.json" --open "$design" > "$work/check.log" 2>&1 || {
  cat "$work/check.log"
  ui_fail 'the saved design does not keep the folders'
}
echo "ok   saved with Rail:1 and Panel:1 in Frame, the bodies in no folder"
ui_finish 'UI Browser folders'
