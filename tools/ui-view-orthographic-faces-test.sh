#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/view/FaceProjection.hpp app/view/CameraHeight.hpp app/view/SpaceMouseView.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/view-orthographic-faces.mitcad-ui
# Perspective with Orthographic Faces (mitcad#2473) through the real UI,
# the shared input script tools/ui-scripts/view-orthographic-faces.mitcad-ui
# (also run by app.ui-windows-view-orthographic-faces and
# app.ui-macos-view-orthographic-faces) on a new design: chosen in View ›
# Camera and saved; a click on the orientation cube's front face logs an
# orthographic camera, an orbit a perspective one with the same height at
# the target; View › Front is orthographic and the arrow keys turn it off
# the face and back; the cube's edge and corner stay perspective;
# Perspective ends the mode.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-orthographic-faces-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100 MITCAD_LOG_ORIENTATION_CUBE=1
export MITCAD_TEST_INPUT=$here/ui-scripts/view-orthographic-faces.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'orthographic faces driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Camera direction 0 1 0 up 0 0 1 (orthographic, perspective with orthographic faces)' \
  'the cube face click gives an orthographic camera'
ui_expect_log 'Camera direction -0.577 0.577 -0.577 up' 'the cube corner click'
ui_expect_log 'TestDriver: done' 'Perspective with Orthographic Faces'
ui_finish 'UI perspective with orthographic faces'
