#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/view/FaceProjection.hpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/view-orthographic-faces.mitcad-ui
# Perspective with Orthographic Faces (mitcad#2473) in the Qt frontend on
# macOS, as tools/ui-view-orthographic-faces-test.sh on Linux: the shared
# steps tools/ui-scripts/view-orthographic-faces.mitcad-ui on a new design.
# Chosen in View › Camera and saved; a click on the orientation cube's front
# face logs an orthographic camera, an orbit a perspective one with the
# same height at the target; View › Front is orthographic and the arrow
# keys turn it off the face and back; the cube's edge and corner stay
# perspective; Perspective ends the mode.
# Usage: tools/ui-macos-view-orthographic-faces-test.sh <app> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-view-orthographic-faces-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/view-orthographic-faces.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  MITCAD_LOG_ORIENTATION_CUBE=1 \
  ui_run_app view-orthographic-faces --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log '[(]orthographic, perspective with orthographic faces[)]$' 'a cube face click gives an orthographic camera'
ui_expect_log '^Camera direction -0[.]577 0[.]577 -0[.]577 up .*[(]perspective, perspective with orthographic faces[)]$' \
  'the cube corner stays perspective'
ui_expect_log '^TestDriver: done' 'Perspective with Orthographic Faces'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt perspective with orthographic faces'
