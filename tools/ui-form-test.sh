#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/commands/FormCage.hpp
# check-all sources: app/view/FormCageOverlay.cpp app/view/FormCageOverlay.hpp app/MainWindowForm.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: app/commands/CommandSupport.cpp app/MainWindow.cpp
# check-all sources: core/subdiv core/model-api/src/form.rs core/model/src/features/form.rs
# check-all sources: tools/ui-scripts/form.mitcad-ui
# Form and Edit Form (mitcad#1757, mitcad#1810) through the real UI, the
# shared input script tools/ui-scripts/form.mitcad-ui (also run by
# tools/ui-windows-form-test.ps1) in an empty design: a Form from a box
# cage, opened from the timeline in Edit Form with its cage drawn over the
# view; a face of the cage picked in the view and moved, then extruded, an
# edge creased semi-sharp and symmetry about the YZ plane picked in the
# Browser, each one undo step whose body is a solid, the last one with its
# halves equal about the plane; a vertex dragged in the view, one undo
# step; Close keeps the edits. Then a Form with Operation Cut cuts a box.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-form-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-form.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/form.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Form driver timed out'; exit 1; }
wait "$UI_RUNNER" || ui_fail 'the Form steps failed'
grep -E '^Form edit (Move|Extrude|Crease|Symmetry|Drag) in |^Form edit Form1: body |^Form cut: ' "$UI_LOG" |
  sed 's/^/     /'
ui_expect_log 'Form edit Move in Form1: done' 'a face of the cage moved, one undo step'
ui_expect_log 'Form edit Extrude in Form1: done' 'the face extruded'
ui_expect_log '"sharpness":1.5' 'an edge creased semi-sharp'
ui_expect_log 'Form edit Symmetry in Form1: done' 'symmetry about the picked plane set'
ui_expect_log 'halves equal about the symmetry plane' 'the symmetric Form'"'"'s halves are equal'
ui_expect_log 'Form edit Drag in Form1: done' 'a vertex dragged in the view, one undo step'
ui_expect_log 'Closed Edit Form after 4' 'Close kept the edits'
ui_expect_log 'Form cut: body ' 'a Form cut from a box'
ui_expect_log 'TestDriver: done' 'a box cage edited into a smooth solid'
# What a person reads names no cage element by its id.
if grep -E '^Panel Edit Form note picks: .*[0-9]+,[0-9]+' "$UI_LOG"; then
  ui_fail 'the panel names a cage element by its ids'
fi
ui_finish 'UI Form'
