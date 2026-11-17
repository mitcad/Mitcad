#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/commands/FormCage.hpp
# check-all sources: app/view/FormCageOverlay.cpp app/MainWindowForm.cpp app/MainWindow.cpp
# check-all sources: app/framework/OpeningEdges.cpp app/framework/TestDriver.cpp
# check-all sources: core/subdiv core/model-api/src/form.rs core/model/src/features/form.rs
# check-all sources: tools/ui-scripts/form-fill-hole.mitcad-ui
# A Form's opening (mitcad#1941) on macOS, as tools/ui-form-fill-hole-test.sh
# on Linux: the shared steps tools/ui-scripts/form-fill-hole.mitcad-ui in an
# empty design in the Qt frontend (a Form from a box cage, a face of its
# top deleted in Edit Form; Patch picks the opening by one free edge, the
# whole loop; Fill Hole from the view's context menu on a picked edge of
# the opening closes the cage again).
# Usage: tools/ui-macos-form-fill-hole-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-form-fill-hole-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
MITCAD_TEST_INPUT=$here/ui-scripts/form-fill-hole.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app form-fill-hole --ui=qt --no-recovery --no-mcp
ui_expect_log 'Form edit Delete Faces in Form1: done' 'a face of the Form deleted, the cage open'
ui_expect_log 'Patch Profiles or Edges: 1 edge' "Patch takes the Form's opening by one free edge"
ui_expect_log 'Context menu: Fill Hole [|] Bridge' 'the context menu of Edit Form offers Fill Hole and Bridge'
ui_expect_log 'Form edit Fill Hole in Form1: done' 'Fill Hole from the context menu, one undo step'
ui_expect_log 'Form edit Form1: cage of 26 vertices, 48 edges, 24 faces, closed' 'the cage closed again'
ui_expect_log '^TestDriver: done' "a Form's opening picked by Patch and filled"
ui_expect_no_log 'TestDriver: FAIL' 'no failed Form Fill Hole assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Form Fill Hole'
