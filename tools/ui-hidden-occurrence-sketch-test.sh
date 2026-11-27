#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser/DocumentSnapshot.cpp app/browser/DocumentSnapshot.hpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# The sketch of a component shown only at its second occurrence
# (mitcad#1194), the shared input script
# tools/ui-scripts/hidden-occurrence-sketch.mitcad-ui (also run by
# tools/ui-windows-hidden-occurrence-sketch-test.ps1): Plate's first
# occurrence lies in a hidden assembly, its second at the root is shown.
# Editing Plate's sketch puts it at the shown occurrence and draws its
# curves there with its dimensions; the hole's diameter set to 12 in its
# value editor changes the plate (the saved design checked).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hidden-occurrence-sketch-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-hidden-occurrence-sketch.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$WORK/workflow.mitcad-ui
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/hidden-occurrence-sketch.mitcad-ui" > "$MITCAD_TEST_INPUT"
"$CLI" run "$here/ui-scripts/hidden-occurrence-sketch.json" --save "$WORK/plate.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/plate.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'hidden occurrence sketch driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "the sketch's curves drawn at the shown occurrence, its hole's diameter edited"
"$CLI" run "$here/ui-scripts/hidden-occurrence-sketch-check.json" --open "$WORK/plate.mitcad" ||
  ui_fail "the saved plate's hole is not 12 in diameter"
ui_finish 'UI sketch of a component shown at its second occurrence'
