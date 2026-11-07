#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandMessage.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/MainWindow.cpp
# check-all sources: tools/ui-scripts/panel-message-top.mitcad-ui tools/ui-scripts/panel-message-top.json tools/cli
# A failed preview's message opens at its first line (mitcad#1430) through
# the real UI, the shared input script
# tools/ui-scripts/panel-message-top.mitcad-ui (also run by
# tools/ui-windows-panel-message-top-test.ps1) on a cube that mitcad-cli
# makes (tools/ui-scripts/panel-message-top.json), in a 1280 x 800 window,
# in the Docked layout and then in the Floating one: Extrude1 edited, its
# distance 0 fails the preview, and the panel's error box shows its text
# from the first line, not scrolled down it ("Panel Extrude message
# <w>x<h>, ..." or "... scrolling, at 0 of <n>, ..."). The box had opened
# scrolled in the Docked layout, its "Failed:" line cut off.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-panel-message-top-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-panel-message-top.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/panel-message-top.mitcad-ui
"$cli" run "$here/ui-scripts/panel-message-top.json" --save "$work/panel-message-top.mitcad" || exit 1
ui_start_display

# run_layout docked|floating: the script in that layout, to its end.
run_layout() {
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1" --open "$work/panel-message-top.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the panel message driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the panel message driver failed"; exit 1; }
  ui_expect_log 'Preview Extrude: failed' "$1: a distance of 0 fails the preview"
  ui_expect_log 'Panel Extrude message box: error: ' "$1: its error in the panel's box"
  grep -E 'Panel Extrude message [0-9]+x[0-9]+' "$UI_LOG" | tail -1
  ui_expect_log 'TestDriver: done' "$1: the error box opens at its first line"
}

run_layout docked
run_layout floating
ui_finish "UI failed preview's message from its first line"
