#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/MainWindow.cpp app/MainWindowDocks.cpp
# check-all sources: app/MainWindowRender.cpp app/MainWindowStart.cpp app/drawings/MainWindowDrawings.cpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp
# check-all sources: app/framework/TitleBar.cpp app/framework/BoostButton.cpp app/agent/MainWindowChat.cpp
# check-all sources: app/framework/WindowStyle.cpp app/DocumentFrame.cpp app/projects/MainWindowProjectsPanel.cpp
# check-all sources: app/render/RenderWorkspace.cpp tools/cli
# check-all sources: tools/ui-scripts/docked-workspaces.mitcad-ui tools/ui-scripts/workspaces.json
# The Docked layout of Windows and Linux (mitcad#2082, GitHub
# mitcad/Mitcad#476) through the real UI, the shared input script
# tools/ui-scripts/docked-workspaces.mitcad-ui (also run by
# tools/ui-windows-docked-workspaces-test.ps1) on a box with one drawing
# that mitcad-cli makes (tools/ui-scripts/workspaces.json): the Floating
# layout's top bar at the window's top (no menu bar row; ☰, Home, File,
# Save, Undo, Redo, the design's tab, Boost, the indicators, Chat,
# Preferences, ? and the version, in that order) and the toolbar panel
# under it (the workspace block, the tabs, the search field at the card's
# row's right end); the Projects panel hidden at the start although the
# old Docked choice says shown; Make's three tabs alone; Render's tab,
# docks and rendered view, Design bringing the tabs, the docks and the view
# mode back; none of Render's docks in the panels' menu outside it; a
# drawing hiding Command, Timeline, Projects and Configuration (the
# Browser stays; not saved as the user's Projects choice) and Close
# Drawing showing exactly those again; Home showing the Start page without
# the panel, the design's tab leading back; the selector locked while
# sketching. (macOS: tools/ui-macos-docked-workspaces-test.sh.)
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-docked-workspaces-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-docked-workspaces.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/docked-workspaces.mitcad-ui
"$cli" run "$here/ui-scripts/workspaces.json" --save "$work/workspaces.mitcad" || exit 1
ui_start_display
# The Docked layout's old choice of the Projects panel (shown): not read
# by the top bar's layout (mitcad#2082).
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[projects]\npanel=true\n' >> "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

ui_start_app --ui=qt --no-mcp --chrome=docked --open "$work/workspaces.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "docked workspaces driver timed out"; exit 1; }
wait "$UI_RUNNER" || ui_fail "the docked workspaces driver failed: $(grep -E 'TestDriver' "$UI_LOG" | tail -8)"
UI_RUNNER=""
ui_expect_log 'Menu bar hidden: Docked' "no menu bar row"
ui_expect_log 'Projects panel at the start: hidden (no choice yet in this layout)' "the Projects panel hidden at the start"
ui_expect_log 'Docked top bar: window 1280x800' "the top bar's parts in the Floating order"
grep -qE '^(Quick access group: flat|Boost button row: |Ribbon tab row end: )' "$UI_LOG" &&
  ui_fail "a stand-in of the old Docked toolbar: $(grep -E '^(Quick access group: flat|Boost button row: |Ribbon tab row end: )' "$UI_LOG" | head -1)"
echo "ok   no quick-access group, Boost or search field in a tab row or menu bar"
ui_expect_log 'Ribbon workspace: MAKE (tabs MAKE_PRINT, MAKE_LASER, MAKE_CNC)' "Make: its three tabs alone"
ui_expect_log 'Ribbon workspace: RENDER (tabs RENDER)' "Render: its tab alone"
ui_expect_log 'Workspace RENDER left: the rendered view off' "Design: the view mode of before"
ui_expect_log 'Workspace DRAWING: opened D1' "Drawing: the design's drawing opened"
ui_expect_log 'Drawing docks hidden: commandDock, projectsDock' "a drawing hides the model's docks shown"
ui_expect_log 'Drawing docks shown again: commandDock, projectsDock' "Close Drawing shows them again"
ui_expect_log 'Docked toolbar panel hidden (Start)' "Home: the Start page without the toolbar panel"
ui_expect_log 'Start page left by the design' "the design's tab leads back"
ui_expect_log 'Docked toolbar panel shown' "the toolbar panel back"
ui_expect_log 'TestDriver: done' "the Docked workspaces"
# The toolbar panel on the screen after the Start page (mitcad#2031):
# the shared script also takes a window not exposed (Windows' session 0).
grep -qE '^Toolbar panel after the Start page: shown at [0-9]+,[0-9]+ [0-9]+x[0-9]+ [(]Docked; the window exposed, ' "$UI_LOG" ||
  ui_fail "the toolbar panel not shown in an exposed window after the Start page: $(grep '^Toolbar panel after the Start page: ' "$UI_LOG" | tail -1)"
echo "ok   the toolbar panel shown in the exposed window after the Start page"
grep -E '^(Docked top bar: |Ribbon workspace: |Drawing docks |TestDriver: dock )' "$UI_LOG" | sed 's/^/     /'
ui_finish 'UI Docked workspaces test'
