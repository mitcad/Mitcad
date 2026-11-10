#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/NavigationBar.cpp app/view/NavigationBar.hpp app/MainWindowNavigationBar.cpp
# check-all sources: app/framework/NavigationBarPins.cpp app/framework/NavigationBarPins.hpp
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/ShortcutDialog.cpp
# check-all sources: app/commands/InspectCommands.cpp app/MainWindow.cpp
# Any command pinned to the navigation bar (mitcad#1589, GitHub
# mitcad/Mitcad#224) through the real UI, in the Floating and then the
# Docked layout, each with settings of its own:
#   1. The bar has Edit Parameters pinned by default, after its separator;
#      the preference (navigationBarPins) is not written yet.
#   2. Measure, a button of SOLID > INSPECT, is pinned from the ⋮ of its
#      row in the INSPECT menu: the row's menu has Pin to Toolbar
#      (unavailable: Measure has its button), Pin to Navigation Bar and
#      Change Keyboard Shortcut…; the bar then has Measure after Edit
#      Parameters, its tooltip the command's, and the preference is saved.
#   3. In the BUILD workspace a click on the bar's Measure starts Measure.
#   4. Measure dragged before Edit Parameters takes the first place.
#   5. The application restarted with the same settings: the bar has
#      Measure and Edit Parameters, in that order.
#   6. Measure's right-click on the bar: Unpin from Navigation Bar and
#      Change Keyboard Shortcut…, which opens Keyboard Shortcuts at
#      Measure; then Unpin from Navigation Bar: the bar without it, its row
#      in the INSPECT menu offering Pin to Navigation Bar again.
#   7. Edit Parameters unpinned from its row in the SOLID > MODIFY menu:
#      the bar without pins (and its separator), saved as an empty value;
#      pinned again from the same row.
# The Windows counterpart is tools/ui-windows-nav-bar-pins-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-nav-bar-pins-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

# pins_saved "ids" description: the settings' navigationBarPins is the ids
# ("" an empty value; "<none>" no key).
pins_saved() {
  local conf=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf value="<none>"
  if grep -q '^navigationBarPins=' "$conf" 2> /dev/null; then
    value=$(sed -n 's/^navigationBarPins=//p' "$conf" | tail -1 | tr -d '" ')
  fi
  [ "$value" = "$1" ] || ui_fail "$2: navigationBarPins is '$value', not '$1'"
  echo "ok   $2: navigationBarPins '$1'"
}

# x_of "prefix": the x of the last place logged as "<prefix> at x,y".
x_of() {
  grep -F -- "$1 at " "$UI_LOG" | tail -1 | sed -E 's/.* at ([0-9-]+),[0-9-]+.*/\1/'
}

# row_menu "path" id "entry" "entries": the ⋮ of the command's row in the
# open menu "path", its row's menu with these entries, then the entry.
row_menu() {
  ui_expect_new "Ribbon menu dots $1 $2 at" "the ⋮ of $2's row"
  ui_mark
  ui_step "the ⋮ of $2"                  ui_click_logged "Ribbon menu dots $1 $2"
  ui_expect_new "Toolbar menu $1 $2: $4" "$2's row menu: $4"
  ui_expect_new "Toolbar menu entry $3 at" "its $3"
  ui_step "$3"                             ui_click_logged "Toolbar menu entry $3"
}

# wide: a window wide enough for every SOLID group with its buttons.
wide() {
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
}

ui_start_display
UI_SETTINGS_ROOT=$XDG_CONFIG_HOME

round() {
  local layout=$1 place
  if [ "$layout" = floating ]; then place=card; else place="status bar"; fi
  echo "--- The $layout layout"
  # Settings of the round's own: the default pin each time.
  export XDG_CONFIG_HOME=$UI_SETTINGS_ROOT/$layout
  mkdir -p "$XDG_CONFIG_HOME"

  # 1. Edit Parameters pinned by default.
  ui_start_app --chrome="$layout"
  wide
  ui_expect_log "Navigation bar shown ($place) at " "$layout: the navigation bar is shown ($place)"
  ui_expect_log "Navigation bar pinned: solid.parameters" "$layout: Edit Parameters pinned by default"
  ui_expect_log "Navigation bar Edit Parameters at " "$layout: Edit Parameters on the bar"
  pins_saved "<none>" "$layout: the default is not written"

  # 2. Measure pinned from its row in SOLID > INSPECT.
  ui_mark
  ui_step "the SOLID tab"                  ui_click_logged "Ribbon tab SOLID"
  ui_sync
  ui_mark
  ui_step "open the SOLID INSPECT menu"    ui_click_logged "Ribbon SOLID/INSPECT"
  row_menu "SOLID/INSPECT" inspect.measure "Pin to Navigation Bar" \
    "Pin to Toolbar | Pin to Navigation Bar | Change Keyboard Shortcut…"
  ui_expect_new "Pinned to navigation bar: Measure" "$layout: Measure pinned"
  ui_expect_new "Navigation bar pinned: solid.parameters, inspect.measure" "$layout: Measure after Edit Parameters"
  ui_sync
  ui_expect_new "Navigation bar Measure at " "$layout: Measure's button on the bar"
  [ "$(x_of "Navigation bar Edit Parameters")" -lt "$(x_of "Navigation bar Measure")" ] ||
    ui_fail "$layout: Measure not right of Edit Parameters"
  grep "^Navigation bar tooltips: " "$UI_LOG" | tail -1 | grep -qF "| Measure: Distances, angles" ||
    ui_fail "$layout: Measure's tooltip is not the command's: $(grep "^Navigation bar tooltips: " "$UI_LOG" | tail -1)"
  echo "ok   $layout: Measure's tooltip is the command's"
  pins_saved "solid.parameters,inspect.measure" "$layout: saved"

  # 3. Measure from the bar in the BUILD workspace.
  ui_mark
  ui_step "the BUILD tab"                  ui_click_logged "Ribbon tab BUILD"
  ui_expect_new "Ribbon tab shown: BUILD" "$layout: the BUILD workspace"
  ui_sync
  ui_mark
  ui_step "Measure (bar)"                  ui_click_logged "Navigation bar Measure"
  ui_expect_new "Navigation bar: Measure" "$layout: the bar's Measure clicked"
  ui_expect_new "Command Measure started" "$layout: Measure started in BUILD"
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Measure cancelled" "$layout: Measure cancelled"
  ui_sync

  # 4. Measure dragged before Edit Parameters.
  ui_mark
  ui_step "drag Measure before Edit Parameters" \
    ui_drag_logged "Navigation bar Measure" "Navigation bar Edit Parameters" -12
  ui_expect_new "Navigation bar pinned order: Measure, Edit Parameters" "$layout: Measure dragged first"
  ui_expect_new "Navigation bar pinned: inspect.measure, solid.parameters" "$layout: the bar in the new order"
  pins_saved "inspect.measure,solid.parameters" "$layout: the new order saved"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Command Measure started" && ui_fail "$layout: the drag started Measure"
  echo "ok   $layout: the drag started no command"
  ui_crashed && ui_fail "$layout: the application crashed"
  ui_stop_app

  # 5. Restarted with the same settings.
  ui_start_app --chrome="$layout"
  wide
  ui_expect_log "Navigation bar pinned: inspect.measure, solid.parameters" "$layout: the pins read back after a restart"
  ui_expect_log "Navigation bar Measure at " "$layout: Measure on the bar after a restart"
  ui_sync
  [ "$(x_of "Navigation bar Measure")" -lt "$(x_of "Navigation bar Edit Parameters")" ] ||
    ui_fail "$layout: Measure not left of Edit Parameters after a restart"
  echo "ok   $layout: Measure before Edit Parameters after a restart"

  # 6. Measure's right-click on the bar.
  ui_mark
  ui_step "right-click Measure (bar)"      ui_click_logged "Navigation bar Measure" 3
  ui_expect_new "Navigation bar menu inspect.measure: Unpin from Navigation Bar | Change Keyboard Shortcut…" \
    "$layout: the pinned button's menu"
  ui_expect_new "Navigation bar menu entry Change Keyboard Shortcut… at" "its Change Keyboard Shortcut…"
  ui_step "Change Keyboard Shortcut…"      ui_click_logged "Navigation bar menu entry Change Keyboard Shortcut…"
  ui_expect_new "Keyboard Shortcuts at Measure" "$layout: Keyboard Shortcuts at Measure"
  ui_focus_dialog '^Keyboard Shortcuts$'
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_focus_main
  ui_sync
  ui_mark
  ui_step "right-click Measure (bar)"      ui_click_logged "Navigation bar Measure" 3
  ui_expect_new "Navigation bar menu entry Unpin from Navigation Bar at" "its Unpin from Navigation Bar"
  ui_step "Unpin from Navigation Bar"      ui_click_logged "Navigation bar menu entry Unpin from Navigation Bar"
  ui_expect_new "Unpinned from navigation bar: Measure" "$layout: Measure unpinned"
  ui_expect_new "Navigation bar pinned: solid.parameters" "$layout: Edit Parameters left"
  pins_saved "solid.parameters" "$layout: Measure no longer saved"
  ui_sync
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Navigation bar Measure at " &&
    ui_fail "$layout: Measure still on the bar"
  echo "ok   $layout: Measure gone from the bar"
  ui_mark
  ui_step "the SOLID tab"                  ui_click_logged "Ribbon tab SOLID"
  ui_sync
  ui_mark
  ui_step "open the SOLID INSPECT menu"    ui_click_logged "Ribbon SOLID/INSPECT"
  ui_expect_new "Ribbon menu dots SOLID/INSPECT inspect.measure at" "the ⋮ of Measure's row"
  ui_mark
  ui_step "the ⋮ of Measure"             ui_click_logged "Ribbon menu dots SOLID/INSPECT inspect.measure"
  ui_expect_new "Toolbar menu SOLID/INSPECT inspect.measure: Pin to Toolbar | Pin to Navigation Bar | " \
    "$layout: Measure's row offers Pin to Navigation Bar again"
  ui_step "close the menus (Esc)"          ui_key Escape
  ui_step "close the menus (Esc)"          ui_key Escape
  ui_sync

  # 7. Edit Parameters unpinned and pinned again from its MODIFY row.
  ui_mark
  ui_step "open the SOLID MODIFY menu"     ui_click_logged "Ribbon SOLID/MODIFY"
  row_menu "SOLID/MODIFY" solid.parameters "Unpin from Navigation Bar" \
    "Pin to Toolbar | Unpin from Navigation Bar | Change Keyboard Shortcut…"
  ui_expect_new "Unpinned from navigation bar: Edit Parameters" "$layout: Edit Parameters unpinned"
  ui_expect_new "Navigation bar pinned: none" "$layout: no pins left"
  pins_saved "" "$layout: an empty value, the default not back"
  ui_sync
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Navigation bar Edit Parameters at " &&
    ui_fail "$layout: Edit Parameters still on the bar"
  echo "ok   $layout: Edit Parameters gone from the bar"
  ui_mark
  ui_step "open the SOLID MODIFY menu"     ui_click_logged "Ribbon SOLID/MODIFY"
  row_menu "SOLID/MODIFY" solid.parameters "Pin to Navigation Bar" \
    "Pin to Toolbar | Pin to Navigation Bar | Change Keyboard Shortcut…"
  ui_expect_new "Pinned to navigation bar: Edit Parameters" "$layout: Edit Parameters pinned again"
  ui_expect_new "Navigation bar pinned: solid.parameters" "$layout: Edit Parameters back on the bar"
  ui_sync
  ui_expect_new "Navigation bar Edit Parameters at " "$layout: its button"
  pins_saved "solid.parameters" "$layout: saved again"
  ui_crashed && ui_fail "$layout: the application crashed"
  ui_stop_app
}

round floating
round docked

ui_finish "UI navigation bar pins test"
