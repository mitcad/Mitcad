#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowDocks.cpp app/framework/DockState.cpp app/framework/DockState.hpp
# check-all sources: app/framework/WindowPlace.cpp app/framework/WindowPlace.hpp app/WindowManager.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowStart.cpp app/MainWindowRender.cpp
# check-all sources: app/framework/Ribbon.cpp app/drawings/MainWindowDrawings.cpp
# check-all sources: app/framework/CommandDockWidth.cpp app/projects/MainWindowProjectsPanel.cpp
# check-all sources: app/agent/MainWindowChat.cpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp
# check-all sources: tools/ui-scripts/dock-state-save.mitcad-ui tools/ui-scripts/dock-state-restore.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-render-move.mitcad-ui tools/ui-scripts/dock-state-render-check.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-screen-gone.mitcad-ui tools/ui-scripts/dock-state-default.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-defaults-kept.mitcad-ui
# The Docked layout's docks kept between starts (mitcad#2070, GitHub
# mitcad/Mitcad#451), through starts of MitCad with the input scripts
# tools/ui-scripts/dock-state-*.mitcad-ui:
#   1. save: nothing kept; the Render workspace opened and closed (its
#      workspace selector), its docks in the places of the docks they hide
#      and those back at their sizes after (mitcad#2077); the Command panel dragged wider, the Browser
#      docked at the right, the Timeline floated at a place, the Chat panel
#      shown; kept when MitCad ends ("Dock state saved: ..."). The
#      remembered Command width is then set to another one.
#   2. restore, with the Start page at launch: the page hides the docks
#      and shows them again; the Browser at the right, the Timeline
#      floating at its place, View › Panels › Chat checked; the Command
#      panel as wide as when MitCad ended, not at the remembered width.
#   3. render-move: the Render workspace, chosen in the Docked tab row's
#      workspace selector (mitcad#2082): its docks at their own areas, its
#      list moved; MitCad ends with the workspace open.
#   4. render-check: the docks the workspace hid shown again as kept; the
#      workspace's docks at their own areas again.
#   5. screen-gone: the kept screen of the floating Timeline renamed to one
#      not connected: the Timeline docked, the log names it.
#   6. default: a new window with the Browser at the right and the Timeline
#      docked; Restore Default Layout puts both windows' docks back at
#      once.
#   7. defaults-kept: the next start opens with the default arrangement.
# The Render workspace opens from the Docked layout's workspace selector,
# as in the Floating layout, and closes with its Design (mitcad#2082). The
# same steps run on Windows (tools/ui-windows-dock-state-test.ps1) and
# macOS (tools/ui-macos-dock-state-test.sh).
#
# Runs headless on Xvfb (see ui-test-lib.sh). MitCad is started here, not
# with ui_start_app, which moves the window to the screen's corner once it
# shows; the window's place is kept between the starts too (mitcad#1710).
# Usage: tools/ui-dock-state-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

# run name script: one Docked start of MitCad running
# tools/ui-scripts/dock-state-<script>.mitcad-ui; the app ends with it.
run() {
  local name=$1
  echo "--- $name"
  export MITCAD_TEST_INPUT=$here/ui-scripts/dock-state-$2.mitcad-ui
  "$UI_APP" --no-recovery --ui=qt --no-mcp --chrome=docked > "$UI_LOG" 2>&1 &
  UI_RUNNER=$!
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$name: the dock state driver timed out"
  wait "$UI_RUNNER" ||
    ui_fail "$name: the dock state driver failed: $(grep -E 'TestDriver|Dock state' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$name"
  grep -E '^Dock state|^TestDriver: dock ' "$UI_LOG" | sed 's/^/     /'
}

# The Command panel's width last logged in this start.
command_width() {
  grep -oE '^Command panel width [0-9]+ ' "$UI_LOG" | tail -1 | cut -d" " -f4
}

ui_start_display
rm -f "$CONF"
mkdir -p "$(dirname "$CONF")"
# The Render workspace renders on the processor (as ui-render-workspace-test.sh).
printf '[render]\ndevice=cpu\n' > "$CONF"

run "1. nothing kept: docks moved, floated and shown" save
ui_expect_log 'Dock state saved: ' "1. the docks kept when MitCad ends"
grep -q '^\[dockState\]' "$CONF" || ui_fail "1. no [dockState] in $CONF"
grep -qE '^floating.timelineDock.screen=' "$CONF" || ui_fail "1. the floating Timeline's screen is not kept"
echo "ok   1. kept in the settings, the floating Timeline's frame and screen with it"
width=$(command_width)
[ -n "$width" ] || ui_fail "1. no Command panel width logged"
grep -q '^dockWidth=560$' "$CONF" || ui_fail "1. the remembered Command width was not set to 560"

export MITCAD_START_PAGE=1
run "2. restored, behind the Start page and after it" restore
export MITCAD_START_PAGE=0
restored=$(command_width)
[ -n "$restored" ] && [ "$restored" -ge $((width - 2)) ] && [ "$restored" -le $((width + 2)) ] ||
  ui_fail "2. the Command panel is ${restored:-?} px wide after the restart, not $width (the remembered 560 applied?)"
echo "ok   2. the Command panel is $restored px wide again ($width when MitCad ended)"

run "3. the Render workspace's docks, one moved, MitCad ends in it" render-move
ui_expect_log 'shown at the next start: ' "3. the docks the workspace hid kept as shown"
run "4. the hidden docks shown again, the workspace's at their own areas" render-check

sed -i 's/^\(floating.timelineDock.screen=\).*/\1gone-monitor/' "$CONF"
grep -q '^floating.timelineDock.screen=gone-monitor$' "$CONF" || ui_fail "5. the kept screen was not renamed"
run "5. the floating Timeline's screen gone: docked" screen-gone
run "6. a new window, Restore Default Layout in both" default
run "7. the default arrangement at the next start" defaults-kept
if grep -qE '^floating.timelineDock' "$CONF"; then
  ui_fail "7. a floating Timeline is still kept after Restore Default Layout"
fi
echo "ok   7. nothing floating kept"

ui_finish 'UI Docked dock state test'
