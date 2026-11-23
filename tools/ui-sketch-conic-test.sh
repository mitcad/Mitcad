#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-conic.mitcad-ui tools/ui-scripts/sketch-conic.json
# Conic Curve and Edit Rho (mitcad#2236), the shared input script
# tools/ui-scripts/sketch-conic.mitcad-ui (also run by
# tools/ui-windows-sketch-conic-test.ps1 and
# tools/ui-macos-sketch-conic-test.sh): a conic drawn by three clicks with
# Rho 0.7 typed, then its Rho changed to 0.3 from its right-click menu.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-conic-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-conic.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-conic.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-conic.json" --save "$WORK/conic.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/conic.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch conic driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'a conic drawn with Rho 0.7 and its Rho changed to 0.3 from its menu'
ui_finish 'UI sketch Conic Curve'
