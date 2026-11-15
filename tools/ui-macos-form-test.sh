#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/commands/FormCage.hpp
# check-all sources: app/view/FormCageOverlay.cpp app/view/FormCageOverlay.hpp app/MainWindowForm.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/subdiv core/model-api/src/form.rs core/model/src/features/form.rs
# check-all sources: tools/ui-scripts/form.mitcad-ui
# Form and Edit Form (mitcad#1757, mitcad#1810) on macOS, as
# tools/ui-form-test.sh on Linux: the shared steps
# tools/ui-scripts/form.mitcad-ui in an empty design in the Qt frontend (a
# Form from a box cage, opened from the timeline; a face picked in the view
# and moved, then extruded, an edge creased semi-sharp and symmetry about
# the picked YZ plane set, each one undo step whose body is a solid; a
# vertex dragged in the view; Close keeps the edits; a Form with Operation
# Cut cuts a box).
# Usage: tools/ui-macos-form-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-form-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=400
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
MITCAD_TEST_INPUT=$here/ui-scripts/form.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app form --ui=qt --no-recovery --no-mcp
ui_expect_log 'Form edit Move in Form1: done' 'a face of the cage moved, one undo step'
ui_expect_log 'Form edit Extrude in Form1: done' 'the face extruded'
ui_expect_log '"sharpness":1[.]5' 'an edge creased semi-sharp'
ui_expect_log 'Form edit Symmetry in Form1: done' 'symmetry about the picked plane set'
ui_expect_log 'halves equal about the symmetry plane' "the symmetric Form's halves are equal"
ui_expect_log 'Form edit Drag in Form1: done' 'a vertex dragged in the view, one undo step'
ui_expect_log 'Closed Edit Form after 4' 'Close kept the edits'
ui_expect_log 'Form cut: body ' 'a Form cut from a box'
ui_expect_log '^TestDriver: done' 'a box cage edited into a smooth solid'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Form assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Form and Edit Form'
