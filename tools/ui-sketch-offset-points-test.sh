#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/DrawTools.cpp app/sketch/SketchController.cpp app/sketch/SketchModel.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-offset-points.mitcad-ui tools/ui-scripts/sketch-offset-points.json
# Lines started and ended at the ends of a derived offset of a spline are
# held there as the source moves (mitcad#2278, GitHub mitcad/Mitcad#569),
# through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-offset-points.json: a B-spline arch and its
# offset outside it): the Line tool from the offset's start to the
# spline's start and from the spline's end to the offset's end closes a
# profile with nothing refused, and a drag of the spline's end keeps it
# closed. The steps are tools/ui-scripts/sketch-offset-points.mitcad-ui,
# which tools/ui-windows-sketch-offset-points-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-offset-points-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-offset-points.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-offset-points.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-offset-points.json" --save "$work/offset-points.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/offset-points.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'offset points driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Dragged p7 to ' 'the spline end was dragged'
ui_expect_log 'TestDriver: done' 'lines at a derived offset end are held there as the source moves'
ui_finish 'UI sketch lines held at a derived offset end'
