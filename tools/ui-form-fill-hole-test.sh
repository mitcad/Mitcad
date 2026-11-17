#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/commands/FormCage.hpp
# check-all sources: app/view/FormCageOverlay.cpp app/MainWindowForm.cpp app/MainWindow.cpp
# check-all sources: app/commands/SurfaceCommands.cpp app/framework/OpeningEdges.cpp app/OcctViewer.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/subdiv core/model-api/src/form.rs core/model/src/features/form.rs
# check-all sources: core/model/src/features/patch.rs geometry/src/surfaces.cpp
# check-all sources: tools/ui-scripts/form-fill-hole.mitcad-ui
# A Form's opening (mitcad#1941) through the real UI, the shared input
# script tools/ui-scripts/form-fill-hole.mitcad-ui (also run by
# tools/ui-windows-form-fill-hole-test.ps1 and
# tools/ui-macos-form-fill-hole-test.sh) in an empty design: a Form from a
# box cage, a face of its top deleted in Edit Form; Patch picks the
# opening of the Form's body by one of its free edges, the whole loop;
# then in Edit Form an edge of the opening picked and Fill Hole chosen
# from the view's context menu closes the cage again, one undo step whose
# body is a solid.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-form-fill-hole-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-form-fill-hole.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/form-fill-hole.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Form Fill Hole driver timed out'; exit 1; }
wait "$UI_RUNNER" || ui_fail 'the Form Fill Hole steps failed'
grep -E '^Form edit (Delete Faces|Fill Hole) in |^Form edit Form1: (body|cage) |^Patch Profiles or Edges: |^Context menu: Fill Hole' "$UI_LOG" |
  sed 's/^/     /'
ui_expect_log 'Form edit Delete Faces in Form1: done' 'a face of the Form deleted, the cage open'
ui_expect_log 'Patch Profiles or Edges: 1 edge' 'Patch takes the Form'"'"'s opening by one free edge'
ui_expect_log 'Context menu: Fill Hole | Bridge' 'the context menu of Edit Form offers Fill Hole and Bridge'
ui_expect_log 'Form edit Fill Hole in Form1: done' 'Fill Hole from the context menu, one undo step'
ui_expect_log 'Form edit Form1: cage of 26 vertices, 48 edges, 24 faces, closed' 'the cage closed again'
ui_expect_log 'TestDriver: done' 'a Form'"'"'s opening picked by Patch and filled'
ui_finish 'UI Form Fill Hole'
