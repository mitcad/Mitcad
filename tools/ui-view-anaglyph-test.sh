#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/ViewController.hpp app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/view-anaglyph.mitcad-ui
# check-all sources: app/view/CameraHeight.hpp
# Anaglyph 3D (mitcad#1449) through the real UI, the shared input script
# tools/ui-scripts/view-anaglyph.mitcad-ui (also run by
# tools/ui-windows-view-anaglyph-test.ps1) on a block that mitcad-cli makes
# (tools/ui-scripts/view-anaglyph.json): View › Anaglyph 3D makes the
# camera a stereo one drawn red/cyan and is saved; a click in the view
# still selects a face; Preferences › Display's stereo depth and
# convergence reach the view at once, are saved with OK and put back on
# Cancel; off, the camera is in its projection from before; Orthographic
# ends it. Preferences › Display's focal lengths (mitcad#1717): 35 mm for
# the perspective camera and 50 mm for Anaglyph 3D give 37.8 and 27.0
# degrees, Anaglyph 3D on and off switches between them, and the model's
# height at the target stays.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-anaglyph-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-view-anaglyph.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/view-anaglyph.mitcad-ui
"$cli" run "$here/ui-scripts/view-anaglyph.json" --save "$work/block.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/block.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'anaglyph driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Anaglyph 3D: a stereo camera, picks, its preferences'
ui_finish 'UI Anaglyph 3D'
