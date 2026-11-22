#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/construct-offset-sketch.mitcad-ui tools/ui-scripts/construct-offset-sketch.json
# Offset Plane takes a sketch (mitcad#2167) through the real UI, the shared
# input script tools/ui-scripts/construct-offset-sketch.mitcad-ui (also run
# by tools/ui-windows-construct-offset-sketch-test.ps1 and
# tools/ui-macos-construct-offset-sketch-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/construct-offset-sketch.json): the
# sketch picked in the Browser and its profile clicked in the view are each
# taken as the sketch's plane, and OK makes the plane 15 mm above it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-construct-offset-sketch-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-construct-offset-sketch.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/construct-offset-sketch.mitcad-ui
"$cli" run "$here/ui-scripts/construct-offset-sketch.json" --save "$work/construct-offset-sketch.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/construct-offset-sketch.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'offset plane on a sketch driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "Construction plane: the sketch of F1 taken as its sketch's plane" 'the sketch picked in the Browser was taken'
ui_expect_log "Construction plane: a profile of F1 taken as its sketch's plane" 'the profile clicked in the view was taken'
ui_expect_log 'TestDriver: done' 'Offset Plane on a sketch'
grep -Eq '^Added plane Plane[0-9]+: origin [(][-0-9.e]+, [-0-9.e]+, 15[)], normal [(]0, 0, 1[)]' "$UI_LOG" ||
  ui_fail 'OK did not make the plane 15 mm above the sketch'
echo "ok   OK made the plane 15 mm above the sketch"
ui_finish 'UI Offset Plane on a sketch'
