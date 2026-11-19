#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render app/MainWindowRender.cpp app/MainWindow.cpp app/framework/Ribbon.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/render_settings.rs core/model-base/src/render_settings.rs tools/cli
# check-all sources: tools/ui-scripts/render-workspace.mitcad-ui tools/ui-scripts/render-workspace.json
# The Render workspace (mitcad#2013) through the real UI: the shared input
# script tools/ui-scripts/render-workspace.mitcad-ui (also run by
# tools/ui-windows-render-workspace-test.ps1 and
# tools/ui-macos-render-workspace-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/render-workspace.json), in the Floating layout
# (the toolbar panel's workspaces, mitcad#1990): Render chosen in the
# workspace selector opens the workspace with the live view, its stages
# as the tab row itself with Render and Cancel at its right and the stage's
# actions under it, each stage its own list at the left and card at the
# right; the stage's actions are the toolbar's groups (Live, then Pick and
# Look, Light, Frame or Output: icon buttons with labels, a caption with ˅);
# Light's list chooses the environment's or a light's settings, an appearance from the library goes to a
# body, two shots are saved, renamed and looked through, Check Now
# finds the hidden body, and Render All Shots renders both shots with 4
# samples into the design's folder. Cycles renders on the CPU. A picture
# of each stage in a 1024 x 768 window (render-look.png, render-light.png,
# render-frame.png, render-output.png) is kept in the pictures folder
# (ui-render-workspace-pictures in MITCAD_UI_LOG_DIR or build/).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-workspace-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-render-workspace.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=80
export MITCAD_TEST_INPUT=$here/ui-scripts/render-workspace.mitcad-ui
export MITCAD_RENDER_SAMPLES=16
pictures=${MITCAD_UI_LOG_DIR:-$root/build}/ui-render-workspace-pictures
mkdir -p "$pictures"
export MITCAD_TEST_PICTURES=$pictures
mkdir -p "$UI_CONFIG/Mitcad"
printf '[render]\ndevice=cpu\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
ui_start_display

# run_layout <floating|docked>: the shared script on a fresh design.
run_layout() {
  export MITCAD_TEST_WORK=$work/$1
  mkdir -p "$MITCAD_TEST_WORK"
  "$cli" run "$here/ui-scripts/render-workspace.json" --save "$MITCAD_TEST_WORK/render-workspace.mitcad" ||
    { ui_fail "mitcad-cli ($1)"; exit 1; }
  ui_start_app --ui=qt --no-mcp "--chrome=$1" --open "$MITCAD_TEST_WORK/render-workspace.mitcad"
  for _ in $(seq 1 3000); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "render workspace driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the Render workspace ($1)"
  ui_expect_log 'Render workspace panels: ' "its panels in place of the others ($1)"
  ui_expect_log 'Ribbon tab RENDER: its own row' "the stages as the tab row ($1)"
  ui_expect_log 'Render live pause at ' "Pause in the Live group ($1)"
  ui_expect_log 'Render groups: Live | Pick | Look' "Look's groups after the Live group ($1)"
  ui_expect_log 'Render groups: Live | Output' "Output's group after the Live group ($1)"
  ui_expect_log 'Render panels: Shots to Render | Output' "each stage its own list and card ($1)"
  ui_expect_log 'Render settings field environment.strength at ' "the environment's settings docked ($1)"
  ui_expect_log 'Render light light1 added (point)' "a light from the Light group's Add Light ($1)"
  ui_expect_log 'Render all shots done: 2 of 2' "both shots rendered ($1)"
  for shot in Shot1 Hero; do
    [ -s "$MITCAD_TEST_WORK/render-workspace-$shot.png" ] || ui_fail "no image of $shot ($1)"
  done
  echo "ok   the images of both shots ($1)"
  for picture in look light frame output; do
    [ -s "$pictures/render-$picture.png" ] || ui_fail "no picture render-$picture.png in $pictures"
  done
  echo "ok   the pictures in $pictures"
}

run_layout floating
ui_finish 'UI Render workspace'
