#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/Ribbon.cpp app/framework/Ribbon.hpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp app/sketch/DrawTools.cpp
# check-all sources: tools/ui-scripts/sketch-point-top-level.mitcad-ui
# The sketch Point at the top level of the Create menu, after Lines, and as
# a button of its own in the ribbon's Create group (mitcad#1545, GitHub
# mitcad/Mitcad#200), the shared input script
# tools/ui-scripts/sketch-point-top-level.mitcad-ui (also run by
# tools/ui-windows-sketch-point-top-level-test.ps1): the Create menu's
# entries with Point second and not in Other Geometry, Point chosen there
# adds a point, and the ribbon's Point button adds another.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-point-top-level-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-point-top-level.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-point-top-level.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Point driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: popup entries: Lines | Point | Rectangles | ' 'Point at the top level of the Create menu'
ui_expect_log 'Ribbon button sketch.point at ' 'a Point button in the Create group'
ui_expect_log 'TestDriver: done' 'Point from the Create menu and from its ribbon button'
ui_finish 'UI sketch Point at the top level'
