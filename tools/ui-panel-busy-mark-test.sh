#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BusyMark.cpp app/framework/BusyMark.hpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandPanel.hpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: app/MainWindow.cpp app/commands/ExtrudeCommands.cpp app/commands/ExtrudeDragProxy.cpp
# check-all sources: tools/ui-scripts/panel-busy-mark.mitcad-ui tools/ui-scripts/panel-message-top.json tools/cli
# A preview being computed shown without resizing the panel (mitcad#1907,
# GitHub mitcad/Mitcad#361) through the real UI, the shared input script
# tools/ui-scripts/panel-busy-mark.mitcad-ui (also run by
# tools/ui-windows-panel-busy-mark-test.ps1 and
# tools/ui-macos-panel-busy-mark-test.sh) on a cube that mitcad-cli makes
# (tools/ui-scripts/panel-message-top.json), every feature a preview
# evaluates slowed to 1 s, in the Docked layout and then in the Floating
# one: Extrude1 edited, its distance arrow dragged up; the previews outlast
# the busy mark's delay, so the mark beside OK and Cancel shows and then
# hides, the message line stays hidden, and every busy line from the
# edit's first preview on gives the panel the same size and needed height
# ("Panel Extrude busy …, panel WxH, needs H"). (The "Computing preview…"
# message line made the panel grow and shrink with each preview.)
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-panel-busy-mark-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-panel-busy-mark.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=1000
export MITCAD_TEST_INPUT=$here/ui-scripts/panel-busy-mark.mitcad-ui
"$cli" run "$here/ui-scripts/panel-message-top.json" --save "$work/panel-busy-mark.mitcad" || exit 1
ui_start_display

# run_layout docked|floating: the script in that layout, to its end, then
# the busy lines from the edit's first preview on.
run_layout() {
  local lines sizes
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1" --open "$work/panel-busy-mark.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the busy mark driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the busy mark driver failed"; exit 1; }
  ui_expect_log 'Manipulator Extrude distance dragged to ' "$1: the distance's arrow dragged"
  ui_expect_log 'Panel Extrude busy mark shown after ' "$1: the busy mark shown while the preview computes"
  ui_expect_log 'Panel Extrude busy mark hidden after ' "$1: and hidden once it has ended"
  ui_expect_log 'TestDriver: done' "$1: a preview being computed resizes nothing"
  # From the end of the edit's first preview to the panel's end (Esc).
  lines=$(sed -n '/^Panel Extrude busy \(mark hidden\|ended\) after /,/^Command Extrude cancelled/p' "$UI_LOG" |
    grep -E '^Panel Extrude busy')
  [ "$(printf '%s\n' "$lines" | grep -c '^Panel Extrude busy mark shown after ')" -ge 1 ] ||
    ui_fail "$1: the busy mark did not show during the drag"
  sizes=$(printf '%s\n' "$lines" | sed -n 's/.*, \(panel [0-9]*x[0-9]*, needs [0-9]*\)$/\1/p' | sort -u)
  [ "$(printf '%s\n' "$sizes" | grep -c .)" = 1 ] ||
    ui_fail "$1: the panel's size changed while previews computed: $(printf '%s' "$sizes" | tr '\n' ';')"
  echo "ok   $1: $(printf '%s\n' "$lines" | grep -c .) busy lines, all with $sizes"
}

run_layout docked
run_layout floating
ui_finish "UI a preview being computed shown without resizing the panel"
