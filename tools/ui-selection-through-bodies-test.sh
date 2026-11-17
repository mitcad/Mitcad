#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/PickContext.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/selection-through-bodies.mitcad-ui tools/ui-scripts/selection-through-bodies.json
# The selection through the bodies (mitcad#1956, GitHub mitcad/Mitcad#395)
# through the real UI, the shared input script
# tools/ui-scripts/selection-through-bodies.mitcad-ui (also run by
# app.ui-windows-selection-through-bodies and
# app.ui-macos-selection-through-bodies) on a design that mitcad-cli makes
# (tools/ui-scripts/selection-through-bodies.json): a block and a finished
# sketch on a plane through it; the profile the block hides, selected with
# no command active, and a face of the block turned away are drawn through
# it (the log names them, nothing meshed); with View > Selection Through
# Bodies off, neither.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-selection-through-bodies-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-selection-through-bodies.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/selection-through-bodies.mitcad-ui
"$cli" run "$here/ui-scripts/selection-through-bodies.json" --save "$work/through.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/through.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'selection through bodies driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Selection through the bodies: profile F2/' 'the buried profile drawn through the block'
ui_expect_log 'Selection through the bodies: face F1.b0/' 'the face turned away drawn through the block'
ui_expect_log 'Selection through the bodies: off' 'Selection Through Bodies off: neither'
ui_expect_log 'TestDriver: done' 'the selection through the bodies'
ui_finish 'UI selection through bodies'
