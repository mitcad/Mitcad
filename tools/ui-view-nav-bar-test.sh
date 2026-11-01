#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/NavigationBar.cpp app/view/NavigationBar.hpp app/MainWindowNavigationBar.cpp
# check-all sources: app/view/ViewController.cpp app/view/ViewController.hpp app/framework/FloatingLayout.cpp
# check-all sources: app/framework/FloatingLayout.hpp app/framework/GlassCard.cpp app/MainWindow.cpp
# check-all sources: app/OcctViewer.cpp app/sketch/SketchPalette.cpp app/drawings/MainWindowDrawings.cpp
# The navigation bar at the bottom of the view (mitcad#1086) through the
# real UI, in the Floating and the Docked layout:
#   1. The bar is shown: a capsule card at the bottom centre of the view,
#      above the Timeline's card and between the side cards (Floating), or
#      in the status bar (Docked), with Look At, Fit, Visual Style, Layout
#      Grid and Snap to Grid; the tooltips give the command's name and
#      shortcut (Fit (F6)).
#   2. Snap to Grid in the bar and in the View menu stay in step: the bar's
#      button turns the View menu's action off, the View menu's command
#      (through the command search) turns the bar's button on again.
#      Layout Grid likewise.
#   3. Visual Style: the bar's drop-down lists the styles, Wireframe chosen
#      there is the View menu's style; the View menu's Shaded with Visible
#      Edges Only is the bar's check again.
#   4. Fit in the bar fits the view.
#   5. In a sketch on XZ: the bar is still shown; after Isometric View the
#      bar's Look At looks at the sketch plane (from FRONT, along 0 1 0);
#      the Sketch Palette's Snap to grid is the bar's Snap to Grid.
#   6. Preferences > Display > Show the navigation bar off hides the bar,
#      on shows it again.
# The Windows counterpart is tools/ui-windows-view-nav-bar-test.ps1; the
# pictures of the bar in both layouts and themes:
# tools/ui-view-nav-bar-look.sh.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-nav-bar-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

# states "grid" "snap" "style" description: the bar's last states line, as
# the bar's buttons and the View menu's actions have them.
states() {
  ui_expect_new "Navigation bar states: Layout Grid $1 (View menu $1), Snap to Grid $2 (View menu $2), Visual Style $3" "$4"
}

# bar_in_floating_cards with|without: the last "Floating cards" line, waited
# for, has the bar's card or has not; with it, the card is above the
# Timeline's card and its middle is between the Browser's card and the
# view's right edge.
bar_in_floating_cards() {
  local line="" bar timeline bx by bw bh ty
  for _ in $(seq 1 25); do
    line=$(grep -o "^Floating cards: .*" "$UI_LOG" | tail -1)
    if [ "$1" = with ] && grep -qF "; navigationBar " <<< "$line"; then break; fi
    if [ "$1" = without ] && ! grep -qF "; navigationBar " <<< "$line"; then
      echo "ok   no navigation bar among the Floating cards"
      return 0
    fi
    sleep 0.2
  done
  [ "$1" = with ] || ui_fail "the navigation bar is still among the Floating cards: '$line'"
  grep -qF "; navigationBar " <<< "$line" || ui_fail "no navigation bar among the Floating cards: '$line'"
  bar=$(grep -oE 'navigationBar [0-9]+,[0-9]+ [0-9]+x[0-9]+' <<< "$line")
  timeline=$(grep -oE 'timelineDock [0-9]+,[0-9]+ [0-9]+x[0-9]+' <<< "$line")
  read -r bx by bw bh <<< "$(sed -E 's/navigationBar ([0-9]+),([0-9]+) ([0-9]+)x([0-9]+)/\1 \2 \3 \4/' <<< "$bar")"
  if [ -n "$timeline" ]; then
    ty=$(sed -E 's/timelineDock [0-9]+,([0-9]+) .*/\1/' <<< "$timeline")
    [ $((by + bh)) -le "$ty" ] || ui_fail "the bar ($bar) is not above the Timeline's card ($timeline)"
  fi
  [ "$bh" -le 60 ] || ui_fail "the bar's card is $bh px high: '$bar'"
  echo "ok   the bar's card among the Floating cards: $bar, above $timeline"
}

ui_start_display

# A round in a layout: floating or docked.
round() {
  local layout=$1 place
  if [ "$layout" = floating ]; then place=card; else place="status bar"; fi
  echo "--- The $layout layout"
  ui_start_app --chrome="$layout"
  ui_mark
  ui_expect_log "Navigation bar shown ($place) at " "$layout: the navigation bar is shown ($place)"
  for button in "Look At" "Fit" "Visual Style" "Layout Grid" "Snap to Grid"; do
    ui_expect_log "Navigation bar $button at " "$layout: the bar has $button"
  done
  [ "$layout" = floating ] && bar_in_floating_cards with
  ui_expect_log "Navigation bar tooltips: Look At" "$layout: Look At's tooltip names the command"
  grep "^Navigation bar tooltips: " "$UI_LOG" | tail -1 | grep -qF "| Fit (F6) |" ||
    ui_fail "$layout: Fit's tooltip has no shortcut: $(grep "^Navigation bar tooltips: " "$UI_LOG" | tail -1)"
  echo "ok   $layout: Fit's tooltip gives its shortcut (F6)"
  UI_MARK=0
  states on on "Shaded with Visible Edges Only" "$layout: the grid and the snap on, the default style"

  echo "--- Snap to Grid and Layout Grid in step with the View menu"
  ui_mark
  ui_step "Snap to Grid (bar)"             ui_click_logged "Navigation bar Snap to Grid"
  ui_expect_new "Navigation bar: Snap to Grid" "$layout: the bar's Snap to Grid clicked"
  ui_expect_new "Snap to grid off" "$layout: snap off"
  states on off "Shaded with Visible Edges Only" "$layout: the View menu's Snap to Grid off with the bar's"
  ui_mark
  ui_step "Snap to Grid (View menu command, search)" ui_command "Snap to Grid"
  ui_expect_new "Snap to grid on" "$layout: snap on from the View menu's command"
  states on on "Shaded with Visible Edges Only" "$layout: the bar's Snap to Grid on with the View menu's"
  ui_mark
  ui_step "Layout Grid (bar)"              ui_click_logged "Navigation bar Layout Grid"
  ui_expect_new "Layout grid off" "$layout: the grid off"
  states off on "Shaded with Visible Edges Only" "$layout: the View menu's Layout Grid off with the bar's"
  ui_mark
  ui_step "Layout Grid (View menu command, search)" ui_command "Layout Grid"
  ui_expect_new "Layout grid on" "$layout: the grid on from the View menu's command"
  states on on "Shaded with Visible Edges Only" "$layout: the bar's Layout Grid on with the View menu's"

  echo "--- Visual Style"
  ui_mark
  ui_step "Visual Style (bar)"             ui_click_logged "Navigation bar Visual Style"
  ui_expect_new "Context menu: Shaded | Shaded with Visible Edges Only | Shaded with Hidden Edges | Wireframe | Wireframe with Hidden Edges" \
    "$layout: the bar's drop-down lists the styles"
  ui_step "Wireframe (drop-down)"          ui_menu_choose "Wireframe"
  ui_expect_new "Visual style Wireframe" "$layout: Wireframe from the bar"
  states on on "Wireframe" "$layout: the View menu's style is Wireframe"
  ui_mark
  ui_step "Shaded with Visible Edges Only (View menu command, search)" ui_command "Shaded with Visible Edges Only"
  ui_expect_new "Visual style Shaded with Visible Edges Only" "$layout: the default style from the View menu"
  states on on "Shaded with Visible Edges Only" "$layout: the bar's drop-down follows the View menu"

  echo "--- Fit"
  ui_mark
  ui_step "Fit (bar)"                      ui_click_logged "Navigation bar Fit"
  ui_expect_new "Navigation bar: Fit" "$layout: the bar's Fit clicked"
  ui_expect_new "Fit: everything shown" "$layout: the view fitted"

  echo "--- In a sketch"
  ui_mark
  ui_step "create sketch on XZ"            ui_create_sketch xz
  ui_expect_new "Sketch plane seen from FRONT (along 0 1 0)" "$layout: the XZ sketch seen from FRONT"
  ui_expect_log "Palette paletteGrid at" "$layout: the palette's Snap to grid is laid out"
  ui_mark
  ui_step "isometric view (search)"        ui_command "Isometric View"
  ui_expect_new "Camera direction" "$layout: the view turned away from the sketch" 8
  [ "$(grep -o "^Navigation bar shown .*\|^Navigation bar hidden" "$UI_LOG" | tail -1)" != "Navigation bar hidden" ] ||
    ui_fail "$layout: the bar is hidden in the sketch"
  ui_mark
  ui_step "Look At (bar)"                  ui_click_logged "Navigation bar Look At"
  ui_expect_new "Look at sketch" "$layout: the bar's Look At looks at the sketch"
  ui_expect_new "Sketch plane seen from FRONT (along 0 1 0)" "$layout: the sketch plane seen from FRONT again"
  ui_mark
  ui_step "Snap to grid (palette)"         ui_click_logged "Palette paletteGrid"
  ui_expect_new "Snap to grid off" "$layout: snap off from the palette"
  states on off "Shaded with Visible Edges Only" "$layout: the bar's Snap to Grid off with the palette's"
  ui_mark
  ui_step "Snap to Grid (bar, in the sketch)" ui_click_logged "Navigation bar Snap to Grid"
  ui_expect_new "Snap to grid on" "$layout: snap on from the bar in the sketch"
  states on on "Shaded with Visible Edges Only" "$layout: snap on again"
  ui_mark
  ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
  ui_expect_new "Sketch finished" "$layout: the sketch finished"

  echo "--- The preference"
  ui_mark
  ui_step "Preferences: Display (search)"  ui_command "Preferences: Display"
  ui_expect_new "Preferences opened: Display, " "$layout: Preferences on its Display page"
  ui_focus_dialog '^Preferences$'
  ui_step "Show the navigation bar off (Alt+V)" ui_key alt+v
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
  ui_expect_new "Preferences: navigation bar hidden" "$layout: the preference saved"
  ui_expect_new "Navigation bar hidden" "$layout: the bar hidden"
  [ "$layout" = floating ] && bar_in_floating_cards without
  ui_mark
  ui_step "Preferences: Display (search)"  ui_command "Preferences: Display"
  ui_expect_new "Preferences opened: Display, " "$layout: Preferences on its Display page again"
  ui_focus_dialog '^Preferences$'
  ui_step "Show the navigation bar on (Alt+V)" ui_key alt+v
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
  ui_expect_new "Preferences: navigation bar shown" "$layout: the preference saved"
  ui_expect_new "Navigation bar shown ($place) at " "$layout: the bar shown again"
  [ "$layout" = floating ] && bar_in_floating_cards with
  ui_crashed && ui_fail "$layout: the application crashed"
  ui_stop_app
}

round floating
round docked

ui_finish "UI view navigation bar test"
