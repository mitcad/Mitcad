#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MenuBarReveal.cpp app/framework/MenuBarReveal.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/MainWindow.cpp
# check-all sources: app/DocumentFrame.cpp app/view/ViewController.cpp tools/cli
# The hidden menu bar (mitcad#1620) through the real UI, in the Floating and
# the Docked layout, both with the top bar of Windows and Linux
# (mitcad#1990, mitcad#2082), with the quick-access group hidden too (so
# that the menus are reachable only by the ☰ button and the keys), on a
# saved plate that mitcad-cli makes. In either layout the menu bar never
# shows, whatever the setting (Preferences does not offer it); the ☰ is the
# title row's part titleMenu from the start, before Undo and Redo ("Floating
# cards: ..." and "Docked top bar: ..." name the parts):
#   1. The ☰ button opens the menus: File, Edit, View, Tools, …, Help.
#   2. Alt alone opens the ☰ menu; Esc closes it. Alt+F (File's mnemonic)
#      opens File from the title row's File; Alt+E (Edit's) Edit beside the
#      ☰ menu; the bar never shows ("Menu bar shown" not logged, the view's
#      top stays: at the window's top in the Floating layout, below the top
#      bar and the toolbar panel in the Docked one).
#   3. The old setting view/menuBar=true (the menu bar shown) changes
#      nothing: no menu bar row in either layout.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-menu-bar-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-menu-bar.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/plate.json" --save "$WORK/plate.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# view_top: the 3D view's top in the window, as logged last.
view_top() { grep -o "^View area [0-9-]* [0-9-]*" "$UI_LOG" | tail -1 | awk '{ print $4 }'; }

# parts_line: the top bar's parts as logged last, in either layout.
parts_line() { grep -oE "^(Floating cards|Docked top bar): .*" "$UI_LOG" | tail -1; }

# title_menu layout: the title row's ☰ part, waited for, first in the left
# group (mitcad#2129), before Undo and Redo, which stay in the group while
# the added commands are hidden.
title_menu() {
  local line="" menu undo
  for _ in $(seq 1 25); do
    line=$(parts_line)
    grep -qF "; titleMenu " <<< "$line" && break
    sleep 0.2
  done
  grep -qF "; titleMenu " <<< "$line" || ui_fail "$1: the title row has no ☰ part: '$line'"
  ui_quick_access_place "$1" "$line"
  ui_sync
  menu=$(grep -o "; titleMenu [0-9-]*" <<< "$line" | awk '{ print $3 }' | cut -d, -f1)
  undo=$(grep -o "^Quick access edit.undo at [0-9]*" "$UI_LOG" | tail -1 | awk '{ print $5 }')
  [ -n "$undo" ] && [ "$menu" -lt "$undo" ] || ui_fail "$1: the ☰ part ($menu) is not before Undo ($undo): '$line'"
  echo "ok   $1: the title row's ☰ part is there, before Undo and Redo"
}

# top_bar_steps layout menu-bar-setting: the steps above in that layout,
# with view/menuBar set as given (true: the old setting of a shown bar).
top_bar_steps() {
  local layout=$1 top entries label
  label=$(tr '[:lower:]' '[:upper:]' <<< "${layout:0:1}")${layout:1}
  echo "=== $layout (view/menuBar=$2)"
  mkdir -p "$(dirname "$CONF")"
  printf '[quickAccess]\nvisible=false\n[view]\nmenuBar=%s\n' "$2" > "$CONF"
  cp "$WORK/plate.mitcad" "$WORK/$layout.mitcad"
  ui_start_app --chrome="$layout" --open "$WORK/$layout.mitcad"
  ui_expect_log "Opened $WORK/$layout.mitcad" "$layout: opened"
  ui_expect_log "Quick access toolbar hidden" "$layout: the quick-access group hidden"
  ui_expect_log "Menu bar hidden: $label" "$layout: the window's menu bar hidden"
  ui_sync
  top=$(view_top)
  [ -n "$top" ] || ui_fail "$layout: no view area logged"
  if [ "$layout" = floating ]; then
    [ "$top" = 0 ] || ui_fail "floating: the view's top is $top, not the window's top (a menu bar row above it?)"
    echo "ok   floating: no menu bar, the view at the window's top"
  else
    # Below the top bar and the toolbar panel, nothing else above it.
    local bar
    bar=$(parts_line)
    [[ $bar == "Docked top bar: "* ]] || ui_fail "docked: no top bar logged: '$bar'"
    # Within the tool bar's own margin (4 px in Qt's own style on Linux), not
    # under a menu bar row.
    grep -qE "; titleRow [0-9]+,[0-9] " <<< "$bar" || ui_fail "docked: the title row is not at the window's top: '$bar'"
    echo "ok   docked: no menu bar, the top bar at the window's top"
  fi
  ui_expect_log "Menu bar button at" "$layout: the ☰ button shows"
  title_menu "$layout"

  echo "--- 1. The ☰ button opens the menus"
  ui_mark
  ui_step "the ☰ button"                   ui_click_logged "Menu bar button"
  ui_expect_new "Context menu: File | Edit | View | Tools" "$layout: the ☰ button's menu opened"
  entries=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu: " | tail -1)
  [[ $entries == *" | Help" ]] || ui_fail "$layout: the ☰ menu does not end with Help: $entries"
  echo "ok   $layout: ${entries#Context menu: }"
  ui_step "File"                           ui_menu_choose "File"
  ui_expect_new "Context menu File: New Design" "$layout: the ☰ button's File opened the File menu"
  ui_step "close the menus (Esc Esc)"      ui_key Escape Escape
  ui_sync

  echo "--- 2. The menus' keys"
  ui_mark
  ui_step "Alt alone"                      ui_key alt
  ui_expect_new "Menus by the keys (Alt): ☰" "$layout: Alt opens the ☰ menu"
  ui_expect_new "Context menu: File | Edit | View | Tools" "$layout: the ☰ menu open"
  ui_step "Esc"                            ui_key Escape
  ui_sync
  ui_mark
  ui_step "Alt+F (File's mnemonic)"        ui_key alt+f
  ui_expect_new "Menus by the keys (Alt+F): File" "$layout: Alt+F opens File from the title row"
  ui_expect_new "File menu: " "$layout: the File menu open"
  ui_step "Esc"                            ui_key Escape
  ui_sync
  ui_mark
  ui_step "Alt+E (Edit's mnemonic)"        ui_key alt+e
  ui_expect_new "Menus by the keys (Alt+E): Edit" "$layout: Alt+E opens Edit beside the ☰ menu"
  ui_expect_new "Context menu Edit: " "$layout: the Edit menu open"
  ui_step "close the menus (Esc Esc)"      ui_key Escape Escape
  ui_sync
  if grep -q "^Menu bar shown" "$UI_LOG"; then
    ui_fail "$layout: the menu bar was shown"
  fi
  [ "$(view_top)" = "$top" ] || ui_fail "$layout: the view's top moved from $top to $(view_top)"
  echo "ok   $layout: the menu bar never showed"
  ui_stop_app
}

ui_start_display
top_bar_steps floating false
top_bar_steps docked false
# The old setting of a shown menu bar: no menu bar row either (mitcad#2082).
top_bar_steps docked true
ui_finish "the hidden menu bar"
