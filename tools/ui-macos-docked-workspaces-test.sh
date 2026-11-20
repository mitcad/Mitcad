#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/MainWindow.cpp app/MainWindowDocks.cpp app/MainWindowRender.cpp
# check-all sources: app/MainWindowStart.cpp app/drawings/MainWindowDrawings.cpp app/framework/QuickAccessGroup.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp tools/cli
# check-all sources: app/framework/TitleBar.cpp app/framework/BoostButton.cpp app/agent/MainWindowChat.cpp
# check-all sources: tools/ui-scripts/docked-workspaces-macos.mitcad-ui tools/ui-scripts/workspaces.json
# The Docked layout's workspace selector (mitcad#2082, GitHub
# mitcad/Mitcad#476) in the Qt frontend on macOS, where the system shows
# the menus and the Docked layout keeps its toolbar without the top bar of
# Windows and Linux (tools/ui-docked-workspaces-test.sh): the steps
# tools/ui-scripts/docked-workspaces-macos.mitcad-ui on a box with one drawing
# that mitcad-cli makes (tools/ui-scripts/workspaces.json): the selector
# first in the tab row, the quick-access group flat; Make's three tabs
# alone; Render's tab, docks and rendered view, Design bringing the tabs,
# the docks and the view mode back; none of Render's docks in the panels'
# menu outside it; a drawing hiding the model's docks (the Browser stays)
# and Close Drawing showing exactly those again, also after the Start page;
# the selector locked while sketching.
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
MITCAD_TEST_INPUT=$here/ui-scripts/docked-workspaces-macos.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app docked-workspaces --ui=qt --no-recovery --no-mcp --chrome=docked --open "$design"
ui_expect_log '^Quick access group: flat [(]Docked[)]$' 'the quick-access group flat on the tab row'
ui_expect_log '^Ribbon workspace block: left of the card .*, as high as both rows: yes ' "the workspace block at the toolbar's left"
ui_expect_log '^Quick access leading ribbonToolbarUndo at [0-9]+,[0-9]+: inside the group, first$' 'Undo in the quick-access group'
ui_expect_log '^Ribbon tab row end: (.*, )?quickAccess, boostButton, ribbonSearch$' "the quick-access group, Boost and the search field at the tab row's right end"
ui_expect_log '^Boost button row: the tab row [(]Docked, the system.s menu bar[)]$' "Boost development in the tab row (the system's menu bar)"
ui_expect_log '^Ribbon workspace: MAKE [(]tabs MAKE_PRINT, MAKE_LASER, MAKE_CNC[)]$' 'Make: its three tabs alone'
ui_expect_log '^Ribbon workspace: RENDER [(]tabs RENDER[)]$' 'Render: its tab alone'
ui_expect_log '^Workspace RENDER left: the rendered view off$' 'Design: the view mode of before'
ui_expect_log '^Workspace DRAWING: opened D1$' "Drawing: the design's drawing opened"
ui_expect_log '^Drawing docks hidden: commandDock, projectsDock$' "a drawing hides the model's docks shown"
ui_expect_log '^Drawing docks shown again: commandDock, projectsDock$' 'Close Drawing shows them again'
ui_expect_log '^Docked toolbar on the Start page: tab row only$' 'the Start page: the tab row alone'
ui_expect_log '^Docked toolbar back: workspace block, tab row and groups$' "Solid's tab: the block and the groups back"
ui_expect_log '^TestDriver: done' 'the Docked workspaces'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Docked workspace selector'
