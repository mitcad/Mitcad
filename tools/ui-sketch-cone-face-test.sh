#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowSketch3d.cpp app/MainWindowVisibility.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp app/OcctViewer.cpp
# check-all sources: core/model-api/src/sketch_surface.rs core/model-base/src/developed.rs
# check-all sources: core/model/src/features/developed.rs core/model/src/features/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-cone-face.mitcad-ui tools/ui-scripts/sketch-cone-face.json
# A sketch on a cone's side (mitcad#1853) through the real UI, the shared
# input script tools/ui-scripts/sketch-cone-face.mitcad-ui (also run by
# tools/ui-windows-sketch-cone-face-test.ps1 and
# tools/ui-macos-sketch-cone-face-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/sketch-cone-face.json, a cone of base radius 20
# and height 30): Create Sketch picks the cone's side and starts the sketch
# in the face's development, the layout grid not drawn on it; a rectangle
# typed 20 by 12 from its origin keeps its lengths on the face (each
# line's length on the face logged from the model's `sketch_surface`
# query), its profile is drawn laid onto the face and its two dimensions'
# lines are drawn on the face, bent with it; Finish Sketch ends the sketch
# and the grid is drawn again. The picture of the view with the rectangle
# and its dimensions (sketch-cone-face.png), the design and the app's log
# are kept in UI_PICTURES (default build/ui-sketch-cone-face-pictures).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-cone-face-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
pictures=${UI_PICTURES:-$root/build/ui-sketch-cone-face-pictures}
rm -rf -- "${pictures:?}"
mkdir -p "$pictures"
pictures=$(cd "$pictures" && pwd)
trap '[ -n "${UI_LOG:-}" ] && cp -f "$UI_LOG" "$pictures/sketch-cone-face.log" 2> /dev/null; ui_cleanup' EXIT
export MITCAD_TEST_WORK=$pictures MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-cone-face.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-cone-face.json" --save "$pictures/cone.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$pictures/cone.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch on a cone driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sketch Sketch2 lies on its face in its developed coordinates (cone, radius 20.000 mm, half angle 33.690 deg)' \
  "the cone's side picked in Create Sketch takes the sketch"
ui_expect_log 'Layout grid not drawn: the sketch lies on a curved face' 'no layout grid on the curved face'
ui_expect_log 'Added rectangle 20 x 12 mm at ' 'the rectangle typed 20 by 12 on the face'
ui_expect_log 'laid onto its face' "the rectangle's profile drawn on the face"
ui_expect_log ', 0 straight, ' "the dimensions' lines drawn on the face"
ui_expect_log 'Layout grid drawn on its plane again' 'the grid drawn again after the sketch'
ui_expect_log 'TestDriver: done' 'a sketch on a cone: picked, drawn by typed sizes, measured on the face, finished'
[ -s "$pictures/sketch-cone-face.png" ] && echo "ok   picture $pictures/sketch-cone-face.png" ||
  ui_fail 'the picture of the sketch on the cone was not saved'
ui_finish 'UI sketch on a conical face'
