#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowDocks.cpp app/framework/DockState.cpp app/framework/DockState.hpp
# check-all sources: app/framework/WindowPlace.cpp app/framework/WindowPlace.hpp app/WindowManager.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowStart.cpp app/framework/CommandDockWidth.cpp
# check-all sources: app/projects/MainWindowProjectsPanel.cpp app/agent/MainWindowChat.cpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp
# check-all sources: tools/ui-scripts/dock-state-save.mitcad-ui tools/ui-scripts/dock-state-restore.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-screen-gone.mitcad-ui tools/ui-scripts/dock-state-default.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-defaults-kept.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-render-move.mitcad-ui tools/ui-scripts/dock-state-render-check.mitcad-ui
# check-all sources: app/MainWindowRender.cpp app/framework/Ribbon.cpp
# The Docked layout's docks kept between starts (mitcad#2070, GitHub
# mitcad/Mitcad#451) on macOS, the shared input scripts of
# tools/ui-dock-state-test.sh (Linux) and tools/ui-windows-dock-state-test.ps1
# (Windows), in the Qt frontend:
#   1. save: nothing kept; the Render workspace opened and closed, its
#      docks in the places of the docks they hide and those back at their
#      sizes after; the Command panel dragged wider, the Browser
#      docked at the right, the Timeline floated at a place, the Chat panel
#      shown; kept when MitCad ends. The remembered Command width is then
#      set to another one.
#   2. restore, with the Start page at launch: the page hides the docks
#      and shows them again; the Browser at the right, the Timeline
#      floating at its place, View › Panels › Chat checked; the Command
#      panel as wide as when MitCad ended, not at the remembered width.
#   3. render-move: the Render workspace, opened from the Docked layout's
#      workspace selector (mitcad#2082): its docks at their own areas, its list
#      moved; MitCad ends with the workspace open.
#   4. render-check: the docks the workspace hid shown again as kept; the
#      workspace's docks at their own areas again, the selector's Design closes it.
#   5. screen-gone: the kept screen of the floating Timeline renamed to one
#      not connected: the Timeline docked, the log names it.
#   6. default: a new window with the Browser at the right and the Timeline
#      docked; Restore Default Layout puts both windows' docks back at once.
#   7. defaults-kept: the next start opens with the default arrangement.
# Usage: tools/ui-macos-dock-state-test.sh <app> <output directory>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo "usage: $0 <app> <output directory>" >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=180
export MITCAD_TEST_STEP_MS=100
runs=0

# run description script: one Docked start of MitCad running
# tools/ui-scripts/dock-state-<script>.mitcad-ui; the app ends with it.
run() {
  runs=$((runs + 1))
  echo "--- $1"
  MITCAD_TEST_INPUT=$here/ui-scripts/dock-state-$2.mitcad-ui \
    ui_run_app "dock-state-$runs-$2" --ui=qt --no-recovery --no-mcp --chrome=docked
  ui_expect_log '^TestDriver: done' "$1"
  grep -E '^Dock state|^TestDriver: dock ' "$UI_LOG_FILE" | sed 's/^/     /'
  [ "$UI_FAILURES" = 0 ] || exit 1
}

# The settings file of the runs (an INI file under MITCAD_SETTINGS_DIR).
settings_file() {
  find "$MITCAD_SETTINGS_DIR" -name '*.ini' | head -n 1
}

# The Command panel's width last logged in the last start.
command_width() {
  grep -oE '^Command panel width [0-9]+ ' "$UI_LOG_FILE" | tail -1 | cut -d" " -f4
}

check() {
  if eval "$1"; then ui_pass "$2"; else ui_fail "$2"; exit 1; fi
}

find "$MITCAD_SETTINGS_DIR" -name '*.ini' -exec rm -f {} +
run "1. nothing kept: docks moved, floated and shown" save
ui_expect_log '^Dock state saved: ' "1. the docks kept when MitCad ends"
file=$(settings_file)
check '[ -n "$file" ] && grep -qE "^floating.timelineDock.screen=" "$file"' \
  "1. the floating Timeline's frame and screen kept"
check 'grep -q "^dockWidth=560$" "$file"' "1. the remembered Command width set to 560"
width=$(command_width)
check '[ -n "$width" ]' "1. the Command panel's width logged ($width)"

export MITCAD_START_PAGE=1
run "2. restored, behind the Start page and after it" restore
export MITCAD_START_PAGE=0
restored=$(command_width)
check '[ -n "$restored" ] && [ "$restored" -ge $((width - 2)) ] && [ "$restored" -le $((width + 2)) ]' \
  "2. the Command panel ${restored:-?} px wide again ($width when MitCad ended)"

# The Render workspace renders on the processor (as the Linux test).
grep -q '^\[render\]' "$file" || printf '\n[render]\ndevice=cpu\n' >> "$file"
run "3. the Render workspace's docks, one moved, MitCad ends in it" render-move
ui_expect_log 'shown at the next start: ' "3. the docks the workspace hid kept as shown"
run "4. the hidden docks shown again, the workspace's at their own areas" render-check

sed -i '' 's/^\(floating.timelineDock.screen=\).*/\1gone-monitor/' "$file"
check 'grep -q "^floating.timelineDock.screen=gone-monitor$" "$file"' "5. the kept screen renamed"
run "5. the floating Timeline's screen gone: docked" screen-gone
run "6. a new window, Restore Default Layout in both" default
run "7. the default arrangement at the next start" defaults-kept
check '! grep -qE "^floating.timelineDock" "$file"' "7. nothing floating kept"

[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt: the Docked layout'"'"'s docks kept between starts'
