#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp app/MainWindow.cpp app/view tools/cli
# check-all sources: core/model-api/src/sketch.rs core/sketch/src/regions.rs core/sketch/src/def.rs
# check-all sources: tools/ui-scripts/sketch-face-outline.mitcad-ui tools/ui-scripts/sketch-face-outline.json
# A sketch on a planar face starts with the face's outline, and Convert to
# Sketch Geometry takes a profile (mitcad#2051, GitHub mitcad/Mitcad#426),
# through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-face-outline.json: a 60 x 40 x 20 block):
# Create Sketch on its top face shows the linked outline; a click inside
# selects its profile, Convert to Sketch Geometry frees the outline (a
# dragged edge moves) and one undo restores it; a profile of ordinary
# curves gets no Convert in the context menu; with the preference off a
# new sketch on the face starts empty, and Convert on the face's region
# frees the outline in one undo step. The steps are
# tools/ui-scripts/sketch-face-outline.mitcad-ui, which
# tools/ui-windows-sketch-face-outline-test.ps1 runs on Windows and
# tools/ui-macos-sketch-face-outline-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-face-outline-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
# The preference's own default, on (the library turns it off).
export MITCAD_SKETCH_PROJECT_FACE=1
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-face-outline.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-face-outline.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-face-outline.json" --save "$work/block.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/block.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch face outline driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "starts with the face's outline: " 'a new sketch on a face starts with its outline'
ui_expect_log 'Undo: Convert to Sketch Geometry in Sketch2' 'the converted outline restored by one undo'
ui_expect_log 'Preferences: project the face' 'the preference turned off'
ui_expect_log 'TestDriver: done' 'outline, Convert of its profile, no Convert of ordinary curves, the preference off'
ui_finish 'UI sketch face outline'
