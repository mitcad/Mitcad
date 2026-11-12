#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserPanel.hpp app/browser/BrowserController.cpp
# check-all sources: app/browser/BrowserPreferences.cpp app/browser/BrowserPreferences.hpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/browser-used-sketches.mitcad-ui tools/ui-scripts/browser-used-sketches.json
# Show used sketches under their features (mitcad#1699) through the real
# UI, the shared input script tools/ui-scripts/browser-used-sketches.mitcad-ui
# (also run by tools/ui-windows-browser-used-sketches-test.ps1) on a design
# that mitcad-cli makes (tools/ui-scripts/browser-used-sketches.json): with
# the preference off (the default) the Browser is as before; turned on in
# Preferences it gets a Features folder at once, Extrude1 and Extrude2 each
# with Sketch1 under them and its shared mark, Extrude3 with Sketch3 and no
# mark, Sketch2 (no feature uses it) left in Sketches; a click on one of
# Sketch1's rows selects both; a feature row's menu is the timeline's; Edit
# Sketch from Sketch1's row under Extrude2 opens Sketch1; turned off, the
# rows are as before again.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-used-sketches-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-used-sketches.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/browser-used-sketches.mitcad-ui
"$cli" run "$here/ui-scripts/browser-used-sketches.json" --save "$work/used.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/used.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'browser used sketches driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Browser used sketches under their features: on' 'the preference turned on in Preferences, applied at once'
ui_expect_log 'Browser shared Root/Features/Extrude1/Sketch1: used by 2 features' 'Sketch1 under Extrude1, marked shared'
ui_expect_log 'Browser shared Root/Features/Extrude2/Sketch1: used by 2 features' 'and under Extrude2, marked shared'
# Anchored: the driver's own echo of the script's expect-no-log line holds
# the same text.
grep -q '^Browser shared Root/Features/Extrude3/' "$UI_LOG" && ui_fail 'Sketch3, used by Extrude3 alone, is marked shared'
echo 'ok   Sketch3 under Extrude3, unmarked'
ui_expect_log 'Editing sketch F1 (Sketch1)' 'Edit Sketch from the row under Extrude2 opens Sketch1'
ui_expect_log 'Browser used sketches under their features: off' 'turned off again'
ui_expect_log 'TestDriver: done' 'the Browser shows used sketches under their features and goes back'
ui_finish 'UI browser used sketches'
