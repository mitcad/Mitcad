#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/framework/CutTargetsPanel.hpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandPanel.hpp app/framework/CommandSession.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/TestDriver.cpp geometry/src/sweep.cpp
# check-all sources: tools/ui-scripts/extrude-cut-unreached.mitcad-ui tools/ui-scripts/extrude-cut-unreached.json tools/cli
# An Extrude Cut whose preview fails before its cut's targets are found
# (mitcad#2118, GitHub mitcad/Mitcad#492) through the real UI, the shared
# input script tools/ui-scripts/extrude-cut-unreached.mitcad-ui (also run by
# tools/ui-windows-extrude-cut-unreached-test.ps1 and
# tools/ui-macos-extrude-cut-unreached-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/extrude-cut-unreached.json: a circle cut up to a
# face in its own plane), in a 1280 x 600 window, Docked and then Floating:
# Extrude2 edited, Operation New Body and Cut again. The Affected
# components line last says "Affected components could not be found: …"
# in words, never "Updating affected components…" once the preview has
# ended, and the panel's error box lies within the page's view ("Panel
# Extrude view y A to B") without the test scrolling to it. Docked, the
# panel is higher than its page (it scrolls). (0.67.0 said "Updating
# affected components…" for good, its error below the page's view.)
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-cut-unreached-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-extrude-cut-unreached.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/extrude-cut-unreached.mitcad-ui
"$cli" run "$here/ui-scripts/extrude-cut-unreached.json" --save "$work/extrude-cut-unreached.mitcad" || exit 1
ui_start_display

# run_layout docked|floating: the script in that layout, to its end, then
# the last places and the list's last line before the panel ended.
run_layout() {
  local lines view top bottom scroll y status
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1" --open "$work/extrude-cut-unreached.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the driver failed"; exit 1; }
  ui_expect_log 'where the sweep starts, which leaves it no length' "$1: the cut's end is reached neither way"
  ui_expect_log 'Cut targets status: Affected components could not be found: ' "$1: the list says why in words"
  ui_expect_log 'TestDriver: done' "$1: the message is brought into view again"
  # Up to the panel's end (Esc).
  lines=$(sed -n '1,/^Command Extrude cancelled/p' "$UI_LOG")
  status=$(printf '%s\n' "$lines" | grep -E '^Cut targets status: ' | tail -1)
  echo "     $status"
  case "$status" in
    'Cut targets status: Affected components could not be found: '*) ;;
    *) ui_fail "$1: the list's last line is not the failure in words: $status" ;;
  esac
  view=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude view y [0-9-]+ to [0-9-]+, ' | tail -1)
  [ -n "$view" ] || ui_fail "$1: the page's view was not logged"
  echo "     $view"
  top=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y \([0-9-]*\) to .*/\1/p')
  bottom=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y [0-9-]* to \([0-9-]*\),.*/\1/p')
  y=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude message at ' | tail -1 | sed -n 's/.* at [0-9-]*,\([0-9-]*\).*/\1/p')
  [ -n "$y" ] || ui_fail "$1: no place logged for the error box"
  { [ "${y:-0}" -ge "${top:-0}" ] && [ "${y:-0}" -le "${bottom:-0}" ]; } ||
    ui_fail "$1: the error box at y $y, outside the page's view (y $top to $bottom)"
  echo "ok   $1: the error box at y $y, within the page's view (y $top to $bottom)"
  if [ "$1" = docked ]; then
    scroll=$(printf '%s\n' "$view" | sed -n 's/.*, scrolled [0-9]* of \([0-9]*\).*/\1/p')
    [ "${scroll:-0}" -gt 0 ] ||
      ui_fail "docked: the panel is not higher than its page at 1280 x 600 (the test needs one that scrolls)"
    echo "ok   docked: the panel scrolls (by up to $scroll px)"
  fi
}

run_layout docked
run_layout floating
ui_finish "UI an Extrude Cut that fails before its targets says why"
