#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowSketch3d.cpp app/MainWindowVisibility.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp app/OcctViewer.cpp
# check-all sources: core/model-api/src/sketch_surface.rs core/model-base/src/developed.rs
# check-all sources: core/model/src/features/developed.rs core/model/src/features/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-cylinder-face.mitcad-ui tools/ui-scripts/sketch-cylinder-face.json
# A sketch on a cylinder's side (mitcad#1768) through the real UI, the
# shared input script tools/ui-scripts/sketch-cylinder-face.mitcad-ui (also
# run by tools/ui-windows-sketch-cylinder-face-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-cylinder-face.json, a cylinder
# of radius 20): Create Sketch picks the cylinder's side and starts the
# sketch in the face's developed coordinates; a rectangle typed 30 by 20
# from its origin is 30 mm around the face and 20 mm along it (each line's
# length on the face logged from the model's `sketch_surface` query), its
# profile is drawn laid onto the face, and Finish Sketch ends the sketch.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-cylinder-face-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-cylinder-face.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-cylinder-face.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-cylinder-face.json" --save "$work/cylinder.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/cylinder.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch on a cylinder driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sketch Sketch2 lies on its face in its developed coordinates (cylinder, radius 20.000 mm)' \
  "the cylinder's side picked in Create Sketch takes the sketch"
ui_expect_log 'Added rectangle 30 x 20 mm at ' 'the rectangle typed 30 by 20 on the face'
ui_expect_log 'laid onto its face' "the rectangle's profile drawn on the face"
ui_expect_log 'TestDriver: done' 'a sketch on a cylinder: picked, drawn by typed sizes, measured on the face, finished'
ui_finish 'UI sketch on a cylindrical face'
