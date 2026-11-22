#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/commands/ProfilePicks.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/CommandSession.cpp tools/cli
# check-all sources: core/sketch/src/outlines.rs core/sketch/src/regions.rs core/model-api/src/lib.rs
# check-all sources: tools/ui-scripts/outline-set.mitcad-ui
# The outline set (mitcad#2194, GitHub mitcad/Mitcad#545) through the real
# UI, the shared input script tools/ui-scripts/outline-set.mitcad-ui (also
# run by tools/ui-windows-outline-set-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/outline-set.json): Show profiles leaves a plate's
# holes unfilled, still hovered and picked; Extrude starts with the plate
# and the island in a hole, takes the same when its sketch is picked on its
# timeline cell or Browser row, and on a sketch whose closed curves cross
# starts with one region and says why, as on a pick of that sketch.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-outline-set-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-outline-set.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/outline-set.mitcad-ui
"$cli" run "$here/ui-scripts/outline-set.json" --save "$work/outline-set.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/outline-set.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'outline set driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Profiles without fill: 2 [F1 r{c' 'Show profiles leaves the holes unfilled'
ui_expect_log 'Idle hover: profile r{c' 'an unfilled hole still hovers'
ui_expect_log 'Profiles: the whole sketch Sketch1 taken as 2 of its outline set' 'a whole sketch picked fills Profiles'
ui_expect_log 'Panel Extrude note: The closed curves of Sketch2 cross each other' 'the crossing sketch says why'
ui_expect_log 'TestDriver: done' 'every step passed'
ui_finish 'UI outline set of nested and crossing sketch regions'
