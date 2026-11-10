#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MenuBarReveal.cpp app/framework/MenuBarReveal.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/MainWindow.cpp
# check-all sources: app/DocumentFrame.cpp app/view/ViewController.cpp tools/cli
# The hidden menu bar (mitcad#1620) through the real UI, in the Floating and
# the Docked layout, with the quick-access group hidden too (so that the
# menus are reachable only by the bar, the ☰ button and the keys), on a
# saved plate that mitcad-cli makes:
#   1. Preferences › Display › Show the menu bar (Alt+E) cleared, OK: saved
#      (view/menuBar=false), the menu bar hidden at once: the 3D view moves
#      up by the bar's height; the ☰ button shows (Floating: the title
#      row's part titleMenu, before Undo and Redo).
#   2. The ☰ button opens the menus: File, Edit, View, Tools, …, Help; its
#      File opens the File menu.
#   3. Alt pressed and released alone shows the menu bar (the view back
#      down), and Esc hides it again; Alt+F (File's mnemonic) shows it too
#      and opens File, and Esc (twice: the first goes back to the bar)
#      hides it again.
#   4. Show the menu bar checked again: the bar back, the ☰ button gone.
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

# expect_view_top above|at value description: waits until the view's top
# is above (less than) the value, or at it.
expect_view_top() {
  local top=""
  for _ in $(seq 1 30); do
    ui_sync
    top=$(view_top)
    if [ "$1" = above ] && [ -n "$top" ] && [ "$top" -lt "$2" ]; then break; fi
    if [ "$1" = at ] && [ "$top" = "$2" ]; then break; fi
    sleep 0.2
  done
  if [ "$1" = above ]; then
    [ -n "$top" ] && [ "$top" -lt "$2" ] || ui_fail "$3: the view's top is $top, not above $2"
  else
    [ "$top" = "$2" ] || ui_fail "$3: the view's top is $top, not $2"
  fi
  echo "ok   $3 (the view's top at $top)"
}

# menu_bar_preference layout shown|hidden: Preferences' Display page, Show
# the menu bar (Alt+E) toggled, OK.
menu_bar_preference() {
  ui_mark
  ui_step "Preferences: Display (search)"  ui_command "Preferences: Display"
  ui_focus_dialog '^Preferences$'
  ui_step "Show the menu bar (Alt+E)"      ui_key alt+e
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
  ui_expect_new "Preferences: menu bar $2" "$1: the preference saved ($2)"
}

# hide_by_esc layout description: Esc (twice where the first one only goes
# back to the bar, as a style that goes to the menu bar on Alt does) hides
# the shown bar again.
hide_by_esc() {
  ui_step "Esc"                            ui_key Escape
  ui_sync
  if ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Menu bar hidden again"; then
    ui_step "Esc (from the bar)"           ui_key Escape
  fi
  ui_expect_new "Menu bar hidden again" "$1: $2"
}

# title_menu with|without: the title row's ☰ part (Floating), waited for.
title_menu() {
  local line=""
  for _ in $(seq 1 25); do
    line=$(grep -o "^Floating cards: .*" "$UI_LOG" | tail -1)
    if [ "$1" = with ] && grep -qF "; titleMenu " <<< "$line"; then break; fi
    if [ "$1" = without ] && ! grep -qF "; titleMenu " <<< "$line"; then break; fi
    sleep 0.2
  done
  if [ "$1" = with ]; then
    grep -qF "; titleMenu " <<< "$line" || ui_fail "the title row has no ☰ part: '$line'"
    # Before Undo and Redo.
    local menu history
    menu=$(grep -o "; titleMenu [0-9-]*" <<< "$line" | awk '{ print $3 }' | cut -d, -f1)
    history=$(grep -o "; titleHistory [0-9-]*" <<< "$line" | awk '{ print $3 }' | cut -d, -f1)
    [ -n "$history" ] && [ "$menu" -lt "$history" ] || ui_fail "the ☰ part is not before Undo and Redo: '$line'"
    echo "ok   the title row's ☰ part is there, before Undo and Redo"
  else
    grep -qF "; titleMenu " <<< "$line" && ui_fail "the title row still has its ☰ part: '$line'"
    echo "ok   the title row's ☰ part is gone"
  fi
}

# menu_bar_steps layout: the four steps above.
menu_bar_steps() {
  local layout=$1 shown hidden entries
  echo "=== $layout"
  mkdir -p "$(dirname "$CONF")"
  printf '[quickAccess]\nvisible=false\n' > "$CONF"
  cp "$WORK/plate.mitcad" "$WORK/$layout.mitcad"
  ui_start_app --chrome="$layout" --open "$WORK/$layout.mitcad"
  ui_expect_log "Opened $WORK/$layout.mitcad" "$layout: opened"
  ui_expect_log "Quick access toolbar hidden" "$layout: the quick-access group hidden"
  ui_sync
  shown=$(view_top)
  [ -n "$shown" ] || ui_fail "$layout: no view area logged"

  echo "--- 1. Show the menu bar cleared"
  menu_bar_preference "$layout" hidden
  grep -q "^menuBar=false" "$CONF" || ui_fail "$layout: view/menuBar is not false in the settings ($CONF)"
  echo "ok   $layout: saved in the settings"
  expect_view_top above "$shown" "$layout: the menu bar hidden, the view moved up"
  hidden=$(view_top)
  [ $((shown - hidden)) -ge 15 ] || ui_fail "$layout: the view moved by $((shown - hidden)) pixels only"
  ui_expect_log "Menu bar button at" "$layout: the ☰ button shows"
  [ "$layout" = floating ] && title_menu with

  echo "--- 2. The ☰ button opens the menus"
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

  echo "--- 3. Alt shows the menu bar until its menu closes"
  ui_mark
  ui_step "Alt alone"                      ui_key alt
  ui_expect_new "Menu bar shown (Alt)" "$layout: Alt shows the menu bar"
  expect_view_top at "$shown" "$layout: the menu bar shown, the view back down"
  hide_by_esc "$layout" "Esc hides it again"
  expect_view_top at "$hidden" "$layout: hidden again, the view up"
  ui_mark
  ui_step "Alt+F (File's mnemonic)"        ui_key alt+f
  ui_expect_new "Menu bar shown (Alt+F)" "$layout: Alt+F shows the menu bar for File"
  ui_expect_new "Menu bar File opened" "$layout: Alt+F opens File from the shown bar"
  hide_by_esc "$layout" "Esc hides it again after File"
  expect_view_top at "$hidden" "$layout: hidden again after File, the view up"

  echo "--- 4. Show the menu bar checked again"
  menu_bar_preference "$layout" shown
  expect_view_top at "$shown" "$layout: the menu bar back, the view where it was"
  grep -q "^menuBar=true" "$CONF" || ui_fail "$layout: view/menuBar is not true in the settings ($CONF)"
  [ "$layout" = floating ] && title_menu without
  ui_stop_app
}

ui_start_display
menu_bar_steps floating
menu_bar_steps docked
ui_finish "the hidden menu bar"
