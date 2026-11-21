#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/occurrence_order.rs
# check-all sources: core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/browser-reorder.json tools/ui-scripts/browser-reorder-check.json
# check-all sources: tools/ui-scripts/browser-reorder.mitcad-ui
# The order of components in the Browser (mitcad#2104) through the real
# UI, the shared input script tools/ui-scripts/browser-reorder.mitcad-ui
# (also run by tools/ui-windows-browser-reorder-test.ps1 and
# tools/ui-macos-browser-reorder-test.sh) on a design mitcad-cli makes
# (tools/ui-scripts/browser-reorder.json): three components.
#   - The third dragged before the first, Move Down on the first row, Undo
#     once; a drop onto another component's folder refused.
#   - Saved and opened again: the order kept (browser-reorder-check.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-reorder-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-reorder.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
design=$work/browser-reorder.mitcad
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/browser-reorder.json" --save "$design" > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli'
}
ui_start_display
export MITCAD_TEST_INPUT=$here/ui-scripts/browser-reorder.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'browser-reorder: driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' 'browser-reorder: the shared steps'
"$cli" run "$here/ui-scripts/browser-reorder-check.json" --open "$design" > "$work/check.log" 2>&1 || {
  cat "$work/check.log"
  ui_fail 'the saved design does not keep the order'
}
echo "ok   saved and opened again: Lid:1, Arm:1, Base:1"
ui_finish 'UI Browser reorder'
