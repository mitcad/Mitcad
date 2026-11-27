#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandPanel.hpp app/framework/CommandMessage.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/panel-message-reveal.mitcad-ui tools/ui-scripts/panel-message-top.json tools/cli
# A new error in a panel higher than its page is scrolled into view by the
# panel itself (mitcad#1884) through the real UI, the shared input script
# tools/ui-scripts/panel-message-reveal.mitcad-ui (also run by
# tools/ui-windows-panel-message-reveal-test.ps1) on a cube that mitcad-cli
# makes (tools/ui-scripts/panel-message-top.json), in a 1280 x 600 window,
# in the Docked layout and then in the Floating one: Extrude1 edited, its
# Distance typed -5 (Symmetric) fails the preview, and the panel's error box is in the
# page's view without the test scrolling to it: its logged place, and the
# Distance field's that keeps the keyboard, lie within "Panel Extrude view
# y A to B". In the Docked layout the box was below the page's view: the
# page must have scrolled (the panel's "message revealed ..., scrolled N"
# with N above 0). (A long Flange panel's refusal had stayed below the
# Docked dock's view, unseen.)
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-panel-message-reveal-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-panel-message-reveal.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/panel-message-reveal.mitcad-ui
"$cli" run "$here/ui-scripts/panel-message-top.json" --save "$work/panel-message-reveal.mitcad" || exit 1
ui_start_display

# expect_in_view layout what pattern lines: the y of the last line of
# `lines` matching `pattern` (… at x,y) lies within the page's view.
expect_in_view() {
  local layout=$1 what=$2 pattern=$3 lines=$4 y
  y=$(printf '%s\n' "$lines" | grep -E "$pattern" | tail -1 | sed -n 's/.* at [0-9-]*,\([0-9-]*\).*/\1/p')
  [ -n "$y" ] || ui_fail "$layout: no place logged for $what"
  { [ "$y" -ge "$top" ] && [ "$y" -le "$bottom" ]; } ||
    ui_fail "$layout: $what at y $y, outside the page's view (y $top to $bottom)"
  echo "ok   $layout: $what at y $y, within the page's view (y $top to $bottom)"
}

# run_layout docked|floating: the script in that layout, to its end, then
# the places logged after the error.
run_layout() {
  local lines view revealed scrolled
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1" --open "$work/panel-message-reveal.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the panel message driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the panel message driver failed"; exit 1; }
  ui_expect_log 'Preview Extrude: failed' "$1: a symmetric distance of -5 fails the preview"
  ui_expect_log 'Panel Extrude message box: error: ' "$1: its error in the panel's box"
  ui_expect_log 'TestDriver: done' "$1: the error is brought into view"
  # From the error to the panel's end (Esc).
  lines=$(sed -n '/^Panel Extrude message box: error: /,/^Command Extrude cancelled/p' "$UI_LOG")
  view=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude view y [0-9-]+ to [0-9-]+, ' | tail -1)
  [ -n "$view" ] || ui_fail "$1: the page's view was not logged after the error"
  echo "     $view"
  top=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y \([0-9-]*\) to .*/\1/p')
  bottom=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y [0-9-]* to \([0-9-]*\),.*/\1/p')
  expect_in_view "$1" "the error box" '^Panel Extrude message at ' "$lines"
  expect_in_view "$1" "the Distance field typed in" '^Panel Extrude input distance at [0-9-]+,[0-9-]+$' "$lines"
  revealed=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude message revealed: ' | tail -1)
  echo "     $revealed"
  if [ "$1" = docked ]; then
    scrolled=$(printf '%s\n' "$revealed" | sed -n 's/.*, scrolled \([0-9]*\) of [0-9]*$/\1/p')
    [ "${scrolled:-0}" -gt 0 ] ||
      ui_fail "docked: the page did not scroll for the error (the box was in view already: the panel is no longer higher than its page at 1280 x 600)"
    echo "ok   docked: the page scrolled $scrolled px to the error by itself"
  fi
}

run_layout docked
run_layout floating
ui_finish "UI a panel's new error scrolled into view"
