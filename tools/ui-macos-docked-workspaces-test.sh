#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/MainWindow.cpp app/MainWindowDocks.cpp
# check-all sources: app/MainWindowRender.cpp app/MainWindowStart.cpp app/drawings/MainWindowDrawings.cpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp
# check-all sources: app/framework/TitleBar.cpp app/framework/BoostButton.cpp app/agent/MainWindowChat.cpp
# check-all sources: app/framework/WindowStyle.cpp app/DocumentFrame.cpp app/projects/MainWindowProjectsPanel.cpp
# check-all sources: app/render/RenderWorkspace.cpp tools/cli
# check-all sources: tools/ui-scripts/docked-workspaces.mitcad-ui tools/ui-scripts/workspaces.json
# The Docked layout's workspace selector (mitcad#2082, GitHub
# mitcad/Mitcad#476) in the Qt frontend on macOS. The shared Qt UI of macOS
# (MITCAD_MACOS_SHARED_QT_UI, docs/macos-shared-qt.md) has the Docked
# layout of Windows and Linux: the Floating layout's top bar at the
# window's top and the toolbar panel under it (MainWindow::createDockedTopBar,
# floatingTopCards()), the system's menu bar holding the menus. So the
# steps are the shared tools/ui-scripts/docked-workspaces.mitcad-ui (also
# run by tools/ui-docked-workspaces-test.sh and
# tools/ui-windows-docked-workspaces-test.ps1), on a box with one drawing
# that mitcad-cli makes (tools/ui-scripts/workspaces.json): the top bar's
# parts in the Floating order; the Projects panel hidden at the start
# although the old Docked choice says shown; Make's three tabs alone;
# Render's tab, docks and rendered view, Design bringing the tabs, the
# docks and the view mode back; none of Render's docks in the panels' menu
# outside it; a drawing hiding the model's docks (the Browser stays) and
# Close Drawing showing exactly those again; Home showing the Start page
# without the panel, the design's tab leading back; the selector locked
# while sketching.
# Usage: tools/ui-macos-docked-workspaces-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-docked-workspaces-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/workspaces.mitcad
"$cli" run "$here/ui-scripts/workspaces.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run workspaces.json' >&2; exit 1; }
# The Docked layout's old choice of the Projects panel (shown), in the
# macOS settings file: not read by the top bar's layout (mitcad#2082).
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '[projects]\npanel=true\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
MITCAD_TEST_INPUT=$here/ui-scripts/docked-workspaces.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app docked-workspaces --ui=qt --no-recovery --no-mcp --chrome=docked --open "$design"
ui_expect_log '^Menu bar hidden: Docked$' 'no menu bar row in the window'
ui_expect_log '^Projects panel at the start: hidden [(]no choice yet in this layout[)]$' \
  'the Projects panel hidden at the start'
ui_expect_log '^Docked top bar: window 1280x800' "the top bar's parts in the Floating order"
if grep -qE '^(Quick access group: flat|Boost button row: |Ribbon tab row end: )' "$UI_LOG_FILE"; then
  ui_fail "a stand-in of the old Docked toolbar: $(grep -E '^(Quick access group: flat|Boost button row: |Ribbon tab row end: )' "$UI_LOG_FILE" | head -1)"
else
  ui_pass 'no quick-access group, Boost or search field in a tab row'
fi
ui_expect_log '^Ribbon workspace: MAKE [(]tabs MAKE_PRINT, MAKE_LASER, MAKE_CNC[)]$' 'Make: its three tabs alone'
ui_expect_log '^Ribbon workspace: RENDER [(]tabs RENDER[)]$' 'Render: its tab alone'
ui_expect_log '^Workspace RENDER left: the rendered view off$' 'Design: the view mode of before'
ui_expect_log '^Workspace DRAWING: opened D1$' "Drawing: the design's drawing opened"
ui_expect_log '^Drawing docks hidden: commandDock, projectsDock$' "a drawing hides the model's docks shown"
ui_expect_log '^Drawing docks shown again: commandDock, projectsDock$' 'Close Drawing shows them again'
ui_expect_log '^Docked toolbar panel hidden [(]Start[)]$' 'Home: the Start page without the toolbar panel'
ui_expect_log '^Start page left by the design' "the design's tab leads back"
ui_expect_log '^Docked toolbar panel shown$' 'the toolbar panel back'
# On the screen (mitcad#2031): the shared script also takes a window not
# exposed (Windows' session 0).
ui_expect_log '^Toolbar panel after the Start page: shown at [0-9]+,[0-9]+ [0-9]+x[0-9]+ [(]Docked; the window exposed, ' \
  'the toolbar panel shown in the exposed window after the Start page'
ui_expect_log '^TestDriver: done' 'the Docked workspaces'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Docked workspace selector'
