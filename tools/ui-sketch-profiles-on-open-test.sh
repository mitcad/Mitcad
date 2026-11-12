#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchPalette.cpp
# check-all sources: app/sketch/SketchPreferences.cpp app/sketch/SketchPreferences.hpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-profiles-on-open.mitcad-ui tools/ui-scripts/sketch-profiles-on-open.json
# Preferences › Sketch › Show profiles when a sketch opens (mitcad#1670,
# GitHub mitcad/Mitcad#255) through the real UI, the shared input script
# tools/ui-scripts/sketch-profiles-on-open.mitcad-ui (also run by
# tools/ui-windows-sketch-profiles-on-open-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-profiles-on-open.json): on (the
# default), a sketch opens with Show profiles checked and its region
# shaded; turned off in Preferences, each sketch opens with it unchecked
# and no region shaded, the palette's checkbox shades it for that sketch
# only, and the next sketch opens with it off again; turned on again,
# sketches open as before.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-profiles-on-open-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-profiles-on-open.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-profiles-on-open.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-profiles-on-open.json" --save "$work/profiles.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/profiles.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch profiles on open driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sketch Sketch1 opens with Show profiles on' 'on (the default): Sketch1 opens with its region shaded'
ui_expect_log 'Preferences: show profiles when a sketch opens off' 'the preference turned off in Preferences › Sketch'
ui_expect_log 'Sketch Sketch1 opens with Show profiles off' 'off: Sketch1 opens with no region shaded'
ui_expect_log 'Show profiles on' 'the palette shades the region for that sketch'
ui_expect_log 'Sketch Sketch2 opens with Show profiles off' 'off: the next sketch opens with no region shaded again'
ui_expect_log 'Preferences: show profiles when a sketch opens on' 'the preference turned on again'
ui_expect_log 'Sketch Sketch2 opens with Show profiles on' 'on again: sketches open as before'
ui_expect_log 'TestDriver: done' 'Show profiles when a sketch opens'
ui_finish 'UI sketch profiles on open'
