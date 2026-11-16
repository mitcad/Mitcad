#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserBackground.cpp app/browser/BrowserBackground.hpp app/browser/BrowserPanel.cpp
# check-all sources: app/browser/BrowserPanel.hpp app/framework/GlassCard.cpp app/framework/FloatingLayout.cpp
# check-all sources: app/MainWindow.cpp app/view/ViewController.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/browser-widths.mitcad-ui tools/ui-scripts/browser-widths-restart.mitcad-ui
# check-all sources: tools/ui-scripts/browser-widths.json
# The Browser's width in the Floating layout (mitcad#1932) through the real
# UI, the shared input scripts tools/ui-scripts/browser-widths.mitcad-ui and
# browser-widths-restart.mitcad-ui (also run by
# tools/ui-windows-browser-widths-test.ps1 and
# tools/ui-macos-browser-widths-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/browser-widths.json). With Rows a long component name
# widens the rows and the card so that it shows in full, a longer one stops
# at a third of the view and is elided (its tooltip has it in full), a
# limit changed in Preferences takes effect at once, the card shrinks back
# and a click beside the rows reaches the view; with Card the dragged width
# grip widens the card and the width comes back after a restart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-widths-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-widths.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_WORK=$work
"$cli" run "$here/ui-scripts/browser-widths.json" --save "$work/browser-widths.mitcad" || exit 1
ui_start_display

# run_script <name>: one of the shared scripts, the app started on the
# design in the Floating layout, with the settings the runs share.
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp --chrome=floating --open "$work/browser-widths.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "browser widths driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the browser widths ($1)"
}

run_script browser-widths
ui_expect_log 'Browser card width 380 remembered' 'the dragged Card width is remembered'
run_script browser-widths-restart
ui_expect_log 'Browser card width 380 (remembered)' 'the Card width comes back after a restart'
ui_finish 'UI Browser widths'
