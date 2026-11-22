#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/ProfilePicks.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/regions.rs core/sketch/src/def.rs core/model/src/features/sketch.rs
# check-all sources: tools/ui-scripts/sketch-own-area.mitcad-ui tools/ui-scripts/sketch-own-area.json
# The face's edges do not split a shape the user closed (mitcad#2197,
# GitHub mitcad/Mitcad#544), through the real UI on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-own-area.json: a block and on
# its top face a circle across the face's edge, and a rectangle past the
# face with two circles inside it): one click in Extrude takes the whole
# disc and the Join adds the whole cylinder; one click in the rectangle
# takes the plate without its holes. The steps are
# tools/ui-scripts/sketch-own-area.mitcad-ui, which
# tools/ui-windows-sketch-own-area-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-own-area-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-own-area.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-own-area.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-own-area.json" --save "$work/own-area.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/own-area.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch own area driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Extrude Profiles: 1 profile [profile r{c1}' 'one click takes the whole disc across the face edge'
ui_expect_log 'TestDriver: done' 'the whole cylinder, and the plate without its holes'
ui_finish 'UI sketch own area'
