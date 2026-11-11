#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dClipboard.cpp app/sketch/SketchClipboard.cpp app/sketch/SketchClipboard.hpp
# check-all sources: app/sketch/Sketch3dTools.cpp app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dPalette.hpp
# check-all sources: app/sketch/Sketch3dController.cpp app/sketch/Sketch3dController.hpp app/MainWindowSketch3d.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch/src/spatial_clipboard.rs
# check-all sources: core/model-api/src/sketch3d.rs core/model-api/src/sketch_clipboard.rs
# check-all sources: tools/ui-scripts/sketch3d-keys-clipboard.mitcad-ui tools/ui-scripts/sketch3d-keys-clipboard.json
# check-all sources: tools/cli
# The 3D sketch manipulator's keys and Copy and Paste between 3D sketches
# (mitcad#1661) through the real UI, the shared input script
# tools/ui-scripts/sketch3d-keys-clipboard.mitcad-ui (also run by
# tools/ui-windows-sketch3d-keys-clipboard-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/sketch3d-keys-clipboard.json): with
# the Line tool, Tab and Shift+Tab cycle the placement plane and Up Arrow
# switches to Normal and back, each logged with its key, the controls'
# tooltips naming the keys; in the fields Tab moves between them; the route
# rises to z 15; copied from 3DSketch1 and pasted into 3DSketch2 it keeps
# its z, its relations and dimensions, as one undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-keys-clipboard-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch3d-keys-clipboard.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch3d-keys-clipboard.mitcad-ui
"$cli" run "$here/ui-scripts/sketch3d-keys-clipboard.json" --save "$work/sketch3d-keys-clipboard.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/sketch3d-keys-clipboard.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail '3D sketch keys and clipboard driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '3D sketch control keys: planes Tab: the next plane, Shift+Tab: the previous plane' \
  'the plane controls'"'"' tooltips name Tab and Shift+Tab'
ui_expect_log '3D placement plane: YZ (Tab)' 'Tab changed the plane'
ui_expect_log '3D placement plane: YZ (Shift+Tab)' 'Shift+Tab changed the plane back'
ui_expect_log '3D placement: Normal (Up)' 'Up Arrow switched to Normal'
ui_expect_log '3D placement: On Plane (Up)' 'Up Arrow switched back to On Plane'
ui_expect_log '3D sketch field focus: dy' 'Tab in a field moved to the next field'
ui_expect_log '3D sketch anchor at p2 (10.000, 0.000, 15.000)' 'the route rose to z 15'
ui_expect_log '3D sketch copied c3 from 3DSketch1: ' 'the line was copied'
ui_expect_log '3D sketch Pasted p1, p2, c3 into 3DSketch2: ' 'it was pasted into the other 3D sketch'
ui_expect_log 'Point p2 at 10.00, 0.00, 15.00 mm' 'the pasted end kept its z'
ui_expect_log 'Undo: Paste in 3DSketch2' 'the paste was one undo step'
ui_expect_log 'TestDriver: done' 'the keys and the clipboard work in a 3D sketch'
ui_finish 'UI 3D sketch keys and clipboard'
