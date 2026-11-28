#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/commands/ExtrudeCommands.cpp app/commands/ProfilePicks.cpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/boundary.rs core/sketch/src/regions.rs core/sketch/src/def.rs core/model/src/features/sketch.rs
# check-all sources: tools/ui-scripts/sketch-profile-boundary.mitcad-ui tools/ui-scripts/sketch-profile-boundary.json
# Profile Boundary (mitcad#2596, after mitcad#2551 and GitHub
# mitcad/Mitcad#712), through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-profile-boundary.json: a block with four bosses
# standing on its top face): Create Sketch on the top face starts with the
# face's outline and the bosses' circles; a rectangle drawn around them is
# one profile; the four circles selected with a window, Profile Boundary makes them holes
# in it and profiles of their own in one undo step (undone and redone);
# Extrude of the rectangle then leaves the footprints clear, and undo takes
# it back. The steps are tools/ui-scripts/sketch-profile-boundary.mitcad-ui.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-profile-boundary-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
# The preference's own default, on (the library turns it off).
export MITCAD_SKETCH_PROJECT_FACE=1
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-profile-boundary.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-profile-boundary.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-profile-boundary.json" --save "$work/bosses.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/bosses.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch profile boundary driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "starts with the face's outline: " 'a new sketch on the face starts with its outline and the circles'
ui_expect_log 'Profile boundary on: ' 'the four circles chosen to bound profiles'
ui_expect_log 'Undo: Profile Boundary in Sketch3' 'the choice undone in one step'
ui_expect_log 'TestDriver: done' 'the Extrude of the rectangle leaves the footprints clear, and is undone'
ui_finish 'UI sketch profile boundary'
