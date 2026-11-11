#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs
# check-all sources: core/model-api/src/sketch_clipboard.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-snap-hold.mitcad-ui tools/ui-scripts/sketch-snap-hold.json
# check-all sources: tools/ui-scripts/sketch-snap-hold-check.json
# Placements held where the snap marker showed (mitcad#1645, GitHub
# mitcad/Mitcad#245), the shared input script
# tools/ui-scripts/sketch-snap-hold.mitcad-ui (also run by
# tools/ui-windows-sketch-snap-hold-test.ps1) on a design mitcad-cli makes
# (tools/ui-scripts/sketch-snap-hold.json): a circle pasted with its
# centre on a point, another moved by Move/Copy's handle onto a point, and
# a Point to Point copy onto a point are held there, and their centres
# follow the points in the saved design
# (tools/ui-scripts/sketch-snap-hold-check.json); a text's frame dragged
# between two points, a 3-Point Arc through a point and a Center Point
# Arc ending on a point are held there too.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-snap-hold-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-snap-hold.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-snap-hold.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-snap-hold.json" --save "$work/hold.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/hold.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'snap hold driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Paste of p10 dropped on point p3' 'the pasted centre is held on the point'
ui_expect_log 'Sketch handle holds p9 where it was dropped: point p5' "the handle's drop holds the centre"
ui_expect_log ' held through point p5' "the 3-Point Arc's middle click holds it through the point"
ui_expect_log 'TestDriver: done' 'pasted, moved, copied, framed and arc points held where they were dropped'
"$cli" run "$here/ui-scripts/sketch-snap-hold-check.json" --open "$work/hold.mitcad" ||
  ui_fail 'the saved holds do not keep the centres on their points'
ui_finish 'UI sketch placements held where the snap marker showed'
