#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserPanel.hpp app/browser/BrowserPreferences.cpp
# check-all sources: app/browser/BrowserPreferences.hpp app/browser/BrowserController.cpp app/view/ViewController.cpp
# check-all sources: app/view/PreferencesRestore.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/browser-control-places.mitcad-ui
# check-all sources: tools/ui-scripts/browser-control-places-docked.mitcad-ui
# check-all sources: tools/ui-scripts/browser-control-places.json
# The Browser rows' Activate radio button and Show eye places (mitcad#2223,
# GitHub mitcad/Mitcad#508) through the real UI, the shared input scripts
# tools/ui-scripts/browser-control-places.mitcad-ui (the Floating layout,
# Rows and Card) and browser-control-places-docked.mitcad-ui (the Docked
# layout), also run by tools/ui-windows-browser-control-places-test.ps1 and
# tools/ui-macos-browser-control-places-test.sh, on a design that mitcad-cli
# makes (tools/ui-scripts/browser-control-places.json): with the defaults
# the radio buttons follow the names; with Activate on Right every radio
# button's centre has the same x at the visible right edge and a click
# activates its component; with the eye on Right too every eye's centre has
# the same x left of them and a click toggles its row; a long name is
# elided before them; Restore Defaults puts both back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-control-places-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-control-places.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_WORK=$work
"$cli" run "$here/ui-scripts/browser-control-places.json" --save "$work/browser-control-places.mitcad" || exit 1
ui_start_display

# run_script <name> <layout>: one of the shared scripts, the app started on
# the design in that layout, with the settings the runs share (the Floating
# run ends with Restore Defaults: both places Classic for the Docked run).
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp "--chrome=$2" --open "$work/browser-control-places.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "browser control places driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the browser control places ($1)"
}

run_script browser-control-places floating
run_script browser-control-places-docked docked
ui_finish 'UI Browser control places'
