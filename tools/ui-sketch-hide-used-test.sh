#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchPreferences.cpp app/sketch/SketchPreferences.hpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriverModel.cpp core/ffi/src/lib.rs core/model/src/document/browser.rs
# check-all sources: tools/ui-scripts/sketch-hide-used.mitcad-ui
# Preferences › Sketch › Hide a sketch when a feature uses it (mitcad#2393,
# GitHub mitcad/Mitcad#652) through the real UI, the input script
# tools/ui-scripts/sketch-hide-used.mitcad-ui, in an empty design: turned
# off, a rectangle sketched on XY and extruded keeps its sketch shown;
# turned on again, a second sketch hides when it is extruded, and the
# first stays shown.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-hide-used-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-hide-used.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-hide-used.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch hide used driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preferences: hide a sketch when a feature uses it off' 'the preference turned off in Preferences › Sketch'
ui_expect_log 'Preferences: hide a sketch when a feature uses it on' 'the preference turned on again'
ui_expect_log 'TestDriver: done' 'off: an extruded sketch stays shown; on again: the next one hides'
ui_finish 'UI sketch hide used'
