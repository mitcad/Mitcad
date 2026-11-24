#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/plane_geometry.rs core/model/src/document/plane_geometry/section.rs
# check-all sources: core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-section-snaps.mitcad-ui tools/ui-scripts/sketch-section-snaps.json
# The drawing tools snap to the centres of circular body edges and to where
# the sketch plane cuts the bodies (mitcad#2324, GitHub mitcad/Mitcad#579),
# through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-section-snaps.json: a cylinder and a sphere,
# Sketch1 on a plane parallel to the cylinder's caps through both): a line
# drawn to the cylinder's axis ends on the centre of its caps' projected
# circle (linked, Coincident); one drawn to the sphere's centre ends on the
# centre of the sphere's linked section; each is one undo step that one
# undo takes away whole; the line's end follows the section when the sphere
# moves. The steps are tools/ui-scripts/sketch-section-snaps.mitcad-ui,
# which tools/ui-windows-sketch-section-snaps-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-section-snaps-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-section-snaps.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-section-snaps.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-section-snaps.json" --save "$work/section.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/section.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch section snaps driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log ', the projected center of E{' 'a line end held on the projected centre of the cylinder caps'
ui_expect_log ', the section center of F2:' 'a line end held on the centre of the sphere section'
ui_expect_log 'TestDriver: done' 'lines to a projected centre and a section centre, undone whole; the end follows the moved sphere'
ui_finish 'UI sketch section snaps'
