#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch_loop.rs core/model-api/src/lib.rs tools/cli
# A double-click on a sketch curve selects its closed loop, else its open
# chain (mitcad#1541, GitHub mitcad/Mitcad#204), the shared input script
# tools/ui-scripts/sketch-double-click-chain.mitcad-ui (also run by
# tools/ui-windows-sketch-double-click-chain-test.ps1): a rectangle side
# selects the rectangle's four lines and not the line leaving its corner,
# that line only itself, an open polyline's line the whole polyline, and
# Ctrl+double-click adds the rectangle to the selection.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-double-click-chain-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-double-click-chain.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-double-click-chain.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/sketch-double-click-chain.json" --save "$WORK/chain.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/chain.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch double-click chain driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'a double-click selects a closed loop past a branch, else an open chain'
ui_finish 'UI sketch double-click chain'
