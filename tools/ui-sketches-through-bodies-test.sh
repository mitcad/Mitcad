#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/browser/BrowserController.cpp app/MainWindowVisibility.cpp app/PickContext.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketches-through-bodies.mitcad-ui tools/ui-scripts/sketches-through-bodies.json
# Sketches through the bodies outside sketch mode (mitcad#1931, GitHub
# mitcad/Mitcad#355) through the real UI, the shared input script
# tools/ui-scripts/sketches-through-bodies.mitcad-ui (also run by
# app.ui-windows-sketches-through-bodies and
# app.ui-macos-sketches-through-bodies) on a design that mitcad-cli makes
# (tools/ui-scripts/sketches-through-bodies.json): a block and a finished
# sketch on a plane through it; the profile the block hides is drawn
# through it (the log names it), a click there in Extrude picks it and
# Extrude joins it to the block; Show All Sketches shows the used sketch
# again; with View > Sketches Through Bodies off it is hidden again; Hide
# All Sketches hides it, one undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketches-through-bodies-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketches-through-bodies.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketches-through-bodies.mitcad-ui
"$cli" run "$here/ui-scripts/sketches-through-bodies.json" --save "$work/through.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/through.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketches through bodies driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sketches through the bodies: F2 4 curve(s)' 'the finished sketch and its profile drawn through the block'
ui_expect_log 'Added extrude (Join)' 'the hidden profile picked and joined to the block'
ui_expect_log 'All sketches Root: shown (1)' 'Show All Sketches shows the used sketch'
ui_expect_log 'Sketches through the bodies: off' 'Sketches Through Bodies off: hidden again'
ui_expect_log 'All sketches Root: hidden (1)' 'Hide All Sketches hides it'
ui_expect_log 'TestDriver: done' 'sketches through the bodies outside sketch mode'
ui_finish 'UI sketches through bodies'
