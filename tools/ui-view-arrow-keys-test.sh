#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/CameraTurn.hpp app/view/Navigation.hpp
# check-all sources: app/OcctViewer.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/view-arrow-keys.mitcad-ui
# The arrow keys turn the view (mitcad#1211) through the real UI, the
# shared input script tools/ui-scripts/view-arrow-keys.mitcad-ui (also run
# by tools/ui-windows-view-arrow-keys-test.ps1) on a block that mitcad-cli
# makes (tools/ui-scripts/view-arrow-keys.json): with the keyboard in the
# 3D view, Right turns the camera round to the right by 15 degrees and Left
# back, Up and Down with a perspective camera, the zoom kept (the camera
# logged), and 7, 1 and Home show the isometric, front and home views
# (mitcad#1684); with the keyboard in Extrude's value field the keys leave
# the view as it is and 7 types the digit.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-arrow-keys-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-view-arrow-keys.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/view-arrow-keys.mitcad-ui
"$cli" run "$here/ui-scripts/view-arrow-keys.json" --save "$work/block.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/block.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'arrow keys driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the arrow keys, Home and 1 to 7 move the view, not from a value field'
ui_finish 'UI arrow keys, Home and 1 to 7 move the view'
