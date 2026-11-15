#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserPanel.hpp app/browser/ComponentColors.hpp
# check-all sources: app/browser/TimelineWidget.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/browser-component-colors.mitcad-ui tools/ui-scripts/browser-component-colors.json
# The Browser's component color bars (mitcad#1876) through the real UI,
# the shared input script tools/ui-scripts/browser-component-colors.mitcad-ui
# (also run by tools/ui-windows-browser-component-colors-test.ps1 and
# tools/ui-macos-browser-component-colors-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/browser-component-colors.json): a
# body in the root, one component placed twice and another once. Every
# component, occurrence and body row logs its component's timeline color
# (the root's rows the section caps' orange), both placements the same;
# the colors follow a rename, a move into another component, a new
# component and its undo, and stay in the dark theme. In the Docked and
# the Floating layout.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-component-colors-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-component-colors.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/browser-component-colors.mitcad-ui
"$cli" run "$here/ui-scripts/browser-component-colors.json" --save "$work/browser-component-colors.mitcad" || exit 1
ui_start_display

# run_layout <name> <chrome>: the shared script in one layout.
run_layout() {
  mkdir -p "$work/$1"
  export MITCAD_TEST_WORK=$work/$1
  ui_start_app --ui=qt --no-mcp "--chrome=$2" --open "$work/browser-component-colors.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "browser component colors driver timed out ($1)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'Browser colors: Root #f29e1a, ' "the root's rows in the default orange ($1)"
  ui_expect_log 'Root/Base:1/Bolt:2 #' "the moved occurrence's rows keep its color ($1)"
  ui_expect_log 'TestDriver: done' "the component color bars ($1)"
}

run_layout docked docked
run_layout floating floating
ui_finish 'UI Browser component color bars'
