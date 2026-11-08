#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/MainWindow.cpp
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/modify.rs core/model-base/src/values.rs core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-own-parameter.mitcad-ui tools/ui-scripts/sketch-own-parameter.json
# A sketch dimension's value that uses the dimension's own parameter keeps
# that parameter as a user parameter (mitcad#1535), through the real UI on
# a design that mitcad-cli makes (tools/ui-scripts/sketch-own-parameter.json:
# the width k5 is BuckleDiameter = 5 mm, the height k6 Strap = 25 mm):
# `Buck` in k5's value editor lists BuckleDiameter as this dimension's
# value, kept as a user parameter; BuckleDiameter * 2 gives k5 a new
# parameter of 10 mm and leaves BuckleDiameter a user parameter at 5 mm,
# as the status bar says; k6's context menu moves Strap to the user
# parameters. The steps are tools/ui-scripts/sketch-own-parameter.mitcad-ui,
# which tools/ui-windows-sketch-own-parameter-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-own-parameter-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-own-parameter.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-own-parameter.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-own-parameter.json" --save "$work/own.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/own.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'own parameter driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'kept as a user parameter)' "the dimension's own parameter is listed as kept"
ui_expect_log 'Dimension k5 = BuckleDiameter * 2 (d' 'the dimension stores BuckleDiameter * 2 in a new parameter'
ui_expect_log 'Sketch notice: BuckleDiameter is now a user parameter (5 mm)' 'the status bar says what moved'
ui_expect_log 'Dimension k6: Strap kept as a user parameter, now Strap' 'Move Strap to User Parameters'
ui_expect_log 'TestDriver: done' 'the own parameter kept as a user parameter, by an expression and by the context menu'
ui_finish 'UI sketch dimension own parameter kept as a user parameter'
