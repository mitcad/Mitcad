#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/MainWindow.cpp app/framework/TestDriver.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/ModelShapes.cpp app/commands/ExtrudeCommands.cpp
# check-all sources: tools/cli tools/ui-scripts/view-mesh-writes.mitcad-ui tools/ui-scripts/view-mesh-writes.json
# The view never meshes the model's shapes itself (mitcad#1553): an edge
# highlight meshed the body's edge on the UI thread while the background
# evaluation of a rolled-back join copied the same edge, which damaged the
# heap (a join crashed later). Through the real UI, the input script
# tools/ui-scripts/view-mesh-writes.mitcad-ui (also run by
# tools/ui-windows-view-mesh-writes-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/view-mesh-writes.json): edges and a face picked
# in Fillet's input and at the idle point while the background evaluation
# runs, the body's edges checked for 3D polygons after each. Then a
# construction plane, a sketch's profile and its curves, which the view
# meshes itself, drawn anew, hovered and picked while the background
# evaluation runs (mitcad#1586): drawn from shapes of the view's own, none
# of the body's, whose edges stay unmeshed.
#
# The check is of the cause, not of a crash: the race itself shows only
# now and then, and ThreadSanitizer does not reliably see it either (the
# shared geometry's atomic reference counts order the two threads for it).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-mesh-writes-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-view-mesh-writes.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=20
export MITCAD_TEST_INPUT=$here/ui-scripts/view-mesh-writes.mitcad-ui
# Each background evaluation 6 s longer: the picks come while one runs
# (Diagnostics.hpp).
export MITCAD_TEST_RECOMPUTE_DELAY_MS='background evaluation=6000'
"$cli" run "$here/ui-scripts/view-mesh-writes.json" --save "$work/view-mesh-writes.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/view-mesh-writes.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'view mesh writes driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log "TestDriver: view's own shapes: " \
  'the profiles, sketch curves and datum planes the view meshes are shapes of its own'
ui_expect_log 'TestDriver: done' 'picks, profiles and planes while the background evaluation runs mesh none of the body'"'"'s edges'
ui_finish 'UI view meshes no model shape'
