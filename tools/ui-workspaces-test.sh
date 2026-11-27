#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/TitleBar.cpp
# check-all sources: app/framework/TitleBar.hpp app/framework/FloatingLayout.cpp app/drawings/MainWindowDrawings.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowStart.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/workspaces.mitcad-ui tools/ui-scripts/workspaces.json
# check-all sources: app/files/ProjectIndicator.cpp app/mcp/AgentIndicator.cpp
# check-all sources: app/framework/MenuBarReveal.cpp app/DocumentFrame.cpp app/drawings/DrawingWorkspace.cpp
# check-all sources: app/resources/layout/default.json
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccessGroup.cpp
# check-all sources: app/drawings/DrawingsPanel.cpp app/browser/BrowserController.cpp
# check-all sources: app/MainWindowRender.cpp app/render/RenderWorkspace.cpp app/framework/GlassCard.cpp
# check-all sources: app/framework/PaintWatch.cpp app/framework/GraphicsEnvironment.cpp app/main.cpp
# The toolbar panel's workspace selector (mitcad#1990) through the real UI,
# the shared input script tools/ui-scripts/workspaces.mitcad-ui (also run
# by tools/ui-windows-workspaces-test.ps1) in the Floating layout, on a box
# with one drawing that mitcad-cli makes (tools/ui-scripts/workspaces.json),
# in a 1024 x 768 window: no menu bar, the ☰ in the top bar, no
# quick-access group there (no commands by default); Design's tabs alone
# at first (Make's in its own workspace), the Start page from the title
# row's Home without the panel and left by the design's tab, then Design ->
# Make (its 3D Print, Laser and CNC tabs alone, each shown) -> Render (its tab alone) -> Drawing (the drawing
# opens, the Drawings card in place of the Browser's: its sheet shown from
# it, + Drawing, Rename) -> Design (it closes) from the selector's list,
# the tab row's tabs logged each time, then the groups that do not fit an
# 820 px window even collapsed in the row's more menu. The pictures of the
# Start page, the Solid tab, the open list, the Make workspace's three tabs,
# the Render and the Drawing workspaces and the narrow window are kept in the test's folder
# (MITCAD_UI_LOG_DIR when check-all gives one).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-workspaces-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-workspaces.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/workspaces.mitcad-ui
"$cli" run "$here/ui-scripts/workspaces.json" --save "$work/workspaces.mitcad" || exit 1
pictures=${MITCAD_UI_LOG_DIR:-$root/build}/ui-workspaces-pictures
mkdir -p "$pictures"
export MITCAD_TEST_WORK=$pictures
ui_start_display

ui_start_app --ui=qt --no-mcp --chrome=floating --open "$work/workspaces.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "workspaces driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Ribbon panel hidden (Start)' "the Start page without the toolbar panel"
ui_expect_log 'Ribbon workspace: MAKE (tabs MAKE_PRINT, MAKE_LASER, MAKE_CNC)' "Design -> Make: its three tabs alone"
ui_expect_log 'Ribbon workspace entry MAKE tabs: 3D Print · Laser · CNC' "Make's entry names its tabs"
ui_expect_log 'Ribbon workspace: RENDER (tabs RENDER)' "Make -> Render: the Render tab alone"
ui_expect_log 'Workspace DRAWING: opened D1' "Render -> Drawing: the design's drawing opened"
ui_expect_log 'Ribbon workspace: DRAWING (tabs DRAWING)' "the Drawing tab alone"
ui_expect_log 'Drawings card: Drawing1 (open, sheet 1 of 1)' "the Drawings card in place of the Browser's"
ui_expect_log 'Renamed Drawing1 to Plan' "the Drawings card's Rename"
# The Drawings card in a column of its own (mitcad#1990): the drawing's bar
# and sheet start right of it, its top level with the bar's.
column=$(grep '^Drawing column at .* beside card ' "$UI_LOG" | tail -1)
[ -n "$column" ] || ui_fail "the drawing's bar and sheet not beside the Drawings card"
read -r cx cy kx ky kw <<< "$(sed -E 's/^Drawing column at (-?[0-9]+),(-?[0-9]+) beside card (-?[0-9]+),(-?[0-9]+) ([0-9]+)x[0-9]+$/\1 \2 \3 \4 \5/' <<< "$column")"
[ -n "${kw:-}" ] && [ "$cx" -ge $((kx + kw)) ] || ui_fail "the drawing's bar and sheet start under the Drawings card: '$column'"
[ "$cy" -eq "$ky" ] || ui_fail "the Drawings card's top is not level with the drawing's bar: '$column'"
echo "ok   the Drawings card in a column of its own, level with the drawing's bar"
# One toolbar panel height in every workspace (mitcad#1990): a switch never
# moves the cards or the view below.
heights=""
for space in DESIGN MAKE RENDER DRAWING; do
  height=$(grep -E "^Ribbon panel height $space: [0-9]+$" "$UI_LOG" | tail -1 | sed -E 's/.*: //')
  [ -n "$height" ] || ui_fail "no toolbar panel height logged in $space"
  heights="$heights $space=$height"
  [ -z "${first:-}" ] && first=$height
  [ "$height" = "$first" ] || ui_fail "the toolbar panel's height differs between the workspaces:$heights"
done
echo "ok   one toolbar panel height in every workspace:$heights"
# The + right after the workspace's tabs (mitcad#1990): the hidden tabs of
# the other workspaces take no room in the row. Render has no + (its stages
# stand in the row in place of the tabs).
gap_re='last tab ends at x (-?[0-9]+), \+ starts at x (-?[0-9]+)$'
for space in DESIGN MAKE DRAWING; do
  gap=$(grep -E "^Ribbon tabs gap $space: " "$UI_LOG" | tail -1)
  [ -n "$gap" ] || ui_fail "no tab row logged in $space"
  [[ $gap =~ $gap_re ]] || ui_fail "no places in the tab row line '$gap'"
  apart=$((BASH_REMATCH[2] - BASH_REMATCH[1]))
  [ "$apart" -ge 0 ] && [ "$apart" -le 16 ] || ui_fail "the + is $apart px from the last tab in $space: '$gap'"
done
echo "ok   the + right after the last tab in Design, Make and Drawing"
# One bottom bar in every workspace (mitcad#1990): Render's Shots bar where
# Design's timeline bar is, as wide and as high (the timeline's last place
# before Render, in the same window size).
bars=$(awk '/^Floating cards: / {
    n = split($0, parts, "; ")
    for (i = 1; i <= n; i++) {
      split(parts[i], f, " ")
      if (f[1] == "timelineDock" && !render) timeline = f[2] " " f[3]
      if (f[1] == "renderBarPanel") { render = 1; shots = f[2] " " f[3] }
    }
  }
  END { print timeline "|" shots }' "$UI_LOG")
[ -n "${bars%%|*}" ] || ui_fail "no timeline bar logged in Design"
[ -n "${bars#*|}" ] || ui_fail "no Shots bar logged in Render"
[ "${bars%%|*}" = "${bars#*|}" ] || ui_fail "the Shots bar is not where the timeline bar is (timeline|shots): $bars"
echo "ok   one bottom bar in Design and Render: ${bars%%|*}"
ui_expect_log 'Workspace DESIGN: the drawing closed' "Drawing -> Design: the drawing closed"
ui_expect_log 'Ribbon more SOLID: none' "the more menu gone in a wide window again"
ui_expect_log 'TestDriver: done' "the workspace selector"
# The toolbar panel on the screen after the Start page (mitcad#2031):
# the shared script also takes a window not exposed (Windows' session 0).
grep -qE '^Toolbar panel after the Start page: shown at [0-9]+,[0-9]+ [0-9]+x[0-9]+ [(]Floating; the window exposed, ' "$UI_LOG" ||
  ui_fail "the toolbar panel not shown in an exposed window after the Start page: $(grep '^Toolbar panel after the Start page: ' "$UI_LOG" | tail -1)"
echo "ok   the toolbar panel shown in the exposed window after the Start page"
ui_expect_toolbar_panel_painted
# The project and assistant indicators in the title row: compact icon
# buttons with a caret, their text in the tooltip (mitcad#1990).
cards=$(grep '^Floating cards: ' "$UI_LOG" | tail -1)
for part in titleIndicator titleAgent; do
  size=$(tr ';' '\n' <<< "${cards#*; }" | sed 's/^ *//' | awk -v name="$part" '$1 == name { print $3 }')
  [ -n "$size" ] || ui_fail "no $part in the title row: '$cards'"
  [ "${size%x*}" -le 48 ] || ui_fail "$part is ${size%x*} px wide, not a compact icon button: '$cards'"
done
echo "ok   the project and assistant indicators are compact icon buttons"
# No menu bar row: the window's menu bar hidden, the title row at the
# window's top.
ui_expect_log 'Menu bar hidden: Floating' "the window's menu bar hidden"
grep -q '^Floating cards: .*; titleRow 0,0 ' <<< "$cards" || ui_fail "the title row is not at the window's top: '$cards'"
echo "ok   no menu bar above the title row"
# The top bar's one left group (mitcad#2129): its default items, at the
# row's left end with the ☰ first in it.
ui_expect_log 'Quick access: menu.file, file.new_design, file.open, file.save' "the left group's default items"
ui_quick_access_place "the title row" "$cards"
for picture in start solid list make-print make-laser make-cnc render drawing narrow; do
  [ -s "$pictures/workspaces-$picture.png" ] || ui_fail "no picture workspaces-$picture.png in $pictures"
done
echo "ok   the pictures in $pictures"
ui_finish 'UI workspace selector'
