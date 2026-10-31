#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/Command.hpp
# check-all sources: app/commands/SketchCommands.cpp app/commands/CreateCommands.cpp
# check-all sources: app/commands/PatternCommands.cpp app/commands/PointPatternCommands.cpp
# check-all sources: app/commands/GearCommands.cpp app/commands/PulleyCommands.cpp app/commands/BearingCommands.cpp
# check-all sources: app/framework/ToolbarPins.cpp app/framework/ToolbarPins.hpp app/framework/ShortcutDialog.cpp
# The sketch CREATE group in submenus (mitcad#222), through the real UI, in
# the Docked and the Floating layout:
#   - the CREATE menu: Line, Rectangles, Circles, Arcs, Other Geometry,
#     Text, Patterns, References (Project, Intersect, Clean Up
#     Projections, Clean Up Stale Projections) and Sketch Dimension, every
#     sketch creation command exactly once; Offset (and Edit Text) in
#     MODIFY, not in CREATE;
#   - split buttons for Rectangles, Circles and Arcs, none for Other
#     Geometry and Patterns: the Circles button starts Center Diameter
#     Circle, its arrow opens the circle variants, which start from there;
#   - the keyboard: the CREATE menu's Circles submenu with the arrow keys,
#     the shortcut C and the command search;
#   - in a narrow window the groups stay within it;
#   - the SOLID CREATE menu in its sections with separators (mitcad#579):
#     Create Sketch; from profiles; on a body's faces; the primitives; the
#     Pattern submenu and Mirror; the Machine Elements submenu (Gear, Gear
#     Pair, Pulley, Belt Drive, Bearing), from which Gear starts by the
#     keyboard.
#   - Pin to Toolbar (mitcad#1001), in both layouts, with a rectangle
#     sketched first (Sweep needs a profile): Sweep pinned from the
#     ⋮ button of its row in the SOLID CREATE menu, the Pinned group at the
#     end of the SOLID tab shows it and it starts Sweep; Loft pinned by the
#     Menu key on its row; Loft dragged before Sweep; Sweep unpinned from
#     its button's right-click, Loft from its row's menu, the group gone;
#     the preference follows each step (toolbarPins/SOLID). Docked also
#     Change Keyboard Shortcut…: Keyboard Shortcuts at Sweep.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-create-menu-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

RECTANGLES="2-Point Rectangle | 3-Point Rectangle | Center Rectangle | 2-Point Rounded Rectangle"
RECTANGLES="$RECTANGLES | Center Rounded Rectangle"
CIRCLES="Center Diameter Circle | 2-Point Circle | 3-Point Circle | 2-Tangent Circle | 3-Tangent Circle"
ARCS="3-Point Arc | Center Point Arc | Tangent Arc"
OTHER="Ellipse | Circumscribed Polygon | Inscribed Polygon | Edge Polygon | Center to Center Slot | Overall Slot"
OTHER="$OTHER | Center Point Slot | Three Point Arc Slot | Center Point Arc Slot | Fit Point Spline"
OTHER="$OTHER | Control Point Spline | Point"
PATTERNS="Rectangular Pattern | Circular Pattern | Mirror"
REFERENCES="Project | Intersect | Clean Up Projections | Clean Up Stale Projections"
CREATE="Line | Rectangles > [$RECTANGLES] | Circles > [$CIRCLES] | Arcs > [$ARCS] | Other Geometry > [$OTHER]"
CREATE="$CREATE | Text > [Text | QR Code] | Patterns > [$PATTERNS] | References > [$REFERENCES] | Sketch Dimension"
# The SOLID CREATE menu (mitcad#579), up to the Machine Elements submenu's
# Bearing: later machine elements may follow it in the submenu.
SOLID_PATTERNS="Rectangular Pattern | Circular Pattern | Pattern on Path | Pattern at Points"
SOLID_CREATE="Create Sketch | - | Extrude | Revolve | Sweep | Loft | Rib | Web | Emboss"
SOLID_CREATE="$SOLID_CREATE | - | Hole | Thread | Plastic Feature"
SOLID_CREATE="$SOLID_CREATE | - | Box | Cylinder | Sphere | Torus | Coil | Helix | Pipe"
SOLID_CREATE="$SOLID_CREATE | - | Pattern > [$SOLID_PATTERNS] | Mirror"
SOLID_CREATE="$SOLID_CREATE | - | Machine Elements > [Gear | Gear Pair | Pulley | Belt Drive | Bearing"

# last_line "prefix": the last log line starting with the prefix, without it.
last_line() {
  grep -F -- "$1" "$UI_LOG" | tail -1 | awk -v p="$1" '{ print substr($0, index($0, p) + length(p)) }'
}

# expect_tool "name" "description": the tool started since the mark, then
# Esc ends it.
expect_tool() {
  ui_expect_new "Sketch tool $1" "$2"
  ui_step "end the tool (Esc)"             ui_key Escape
}

# within_window "description": every group of the SKETCH tab logged last
# is inside the window.
within_window() {
  local width group at x
  width=$(xdotool getwindowgeometry --shell "$UI_WINDOW" | sed -n 's/^WIDTH=//p')
  for group in CREATE MODIFY CONSTRAINTS "FINISH SKETCH"; do
    at=$(grep -F "Ribbon SKETCH/$group at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9]*\),[0-9]*$/\1/p')
    [ -n "$at" ] || ui_fail "$1: SKETCH/$group not logged"
    x=$((at * UI_SCALE))
    [ "$x" -gt 0 ] && [ "$x" -lt "$width" ] || ui_fail "$1: SKETCH/$group at x $x, the window is $width wide"
  done
  echo "ok   $1: the groups within the window ($width px)"
}

check_layout() {
  local layout=$1 modify
  echo "=== $layout"
  ui_mark
  ui_create_sketch
  ui_expect_new "Ribbon SKETCH/CREATE at" "$layout: the CREATE group"
  for subgroup in Rectangles Circles Arcs; do
    ui_expect_new "Ribbon split SKETCH/CREATE/$subgroup at" "$layout: a $subgroup split button"
    ui_expect_new "Ribbon split SKETCH/CREATE/$subgroup arrow at" "$layout: its arrow"
  done
  for subgroup in "Other Geometry" Patterns Lines Text References; do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Ribbon split SKETCH/CREATE/$subgroup " &&
      ui_fail "$layout: a split button for $subgroup"
  done
  echo "ok   $layout: no split buttons for the groups without a default"

  echo "--- The CREATE menu"
  ui_mark
  ui_step "open the CREATE menu"           ui_click_logged "Ribbon SKETCH/CREATE"
  ui_expect_new "Ribbon menu SKETCH/CREATE: " "$layout: the CREATE menu opened"
  [ "$(last_line "Ribbon menu SKETCH/CREATE: ")" = "$CREATE" ] ||
    ui_fail "$layout: the CREATE menu is '$(last_line "Ribbon menu SKETCH/CREATE: ")'"
  echo "ok   $layout: the CREATE menu has every tool once, in its groups"
  # The keyboard: the third entry (Circles), its submenu, its second entry.
  ui_step "Circles > 2-Point Circle"       ui_key Down Down Down Right Down Return
  expect_tool "2-Point Circle" "$layout: 2-Point Circle from the menu by the keyboard"

  echo "--- Offset in MODIFY"
  ui_mark
  ui_step "open the MODIFY menu"           ui_click_logged "Ribbon SKETCH/MODIFY"
  ui_expect_new "Ribbon menu SKETCH/MODIFY: " "$layout: the MODIFY menu opened"
  ui_step "close it (Esc)"                 ui_key Escape
  modify=$(last_line "Ribbon menu SKETCH/MODIFY: ")
  case " | $modify | " in
  *" | Offset | "*) echo "ok   $layout: Offset in MODIFY" ;;
  *) ui_fail "$layout: no Offset in MODIFY: '$modify'" ;;
  esac
  case " | $modify | " in
  *" | Edit Text | "*) echo "ok   $layout: Edit Text in MODIFY" ;;
  *) ui_fail "$layout: no Edit Text in MODIFY: '$modify'" ;;
  esac

  echo "--- The Circles split button"
  ui_mark
  ui_step "click the Circles button"       ui_click_logged "Ribbon split SKETCH/CREATE/Circles"
  expect_tool "Center Diameter Circle" "$layout: the button starts Center Diameter Circle"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Ribbon menu SKETCH/CREATE/Circles: " &&
    ui_fail "$layout: the button opened the menu"
  ui_mark
  ui_step "click its arrow"                ui_click_logged "Ribbon split SKETCH/CREATE/Circles arrow"
  ui_expect_new "Ribbon menu SKETCH/CREATE/Circles: $CIRCLES" "$layout: the arrow shows the circle variants"
  ui_step "choose 3-Point Circle"          ui_key Down Down Down Return
  expect_tool "3-Point Circle" "$layout: a variant from the arrow's menu"
  ui_mark
  ui_step "the Arcs arrow"                 ui_click_logged "Ribbon split SKETCH/CREATE/Arcs arrow"
  ui_expect_new "Ribbon menu SKETCH/CREATE/Arcs: $ARCS" "$layout: the arc variants"
  ui_step "close it (Esc)"                 ui_key Escape

  echo "--- Shortcut and search"
  ui_mark
  ui_step "C"                              ui_key c
  expect_tool "Center Diameter Circle" "$layout: the shortcut C"
  ui_mark
  ui_step "search 3-Point Arc"             ui_command "3-Point Arc"
  expect_tool "3-Point Arc" "$layout: the command search"
  ui_mark
  ui_step "search Offset"                  ui_command "Offset"
  ui_expect_new "Command Offset started" "$layout: Offset from the search"
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Offset cancelled" "$layout: Offset cancelled"
  ui_mark
  ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
  ui_expect_new "Sketch finished" "$layout: sketch left"
}

# pins_saved "ids" description: the settings' toolbarPins/SOLID is the ids
# ("" none).
pins_saved() {
  local conf=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf value
  value=$(sed -n '/^\[toolbarPins\]/,/^\[/p' "$conf" 2> /dev/null | sed -n 's/^SOLID=//p' | tr -d '" ')
  [ "$value" = "$1" ] || ui_fail "$2: toolbarPins/SOLID is '$value', not '$1'"
  echo "ok   $2: toolbarPins/SOLID '$1'"
}

# row_menu "path" id "entry": the ⋮ of the command's row in the open menu
# "path" (logged as the menu shows), then the entry of the row's menu.
row_menu() {
  ui_expect_new "Ribbon menu dots $1 $2 at" "the ⋮ of $2's row"
  ui_mark
  ui_step "the ⋮ of $2"                  ui_click_logged "Ribbon menu dots $1 $2"
  ui_expect_new "Toolbar menu $1 $2: " "$2's row menu"
  ui_expect_new "Toolbar menu entry $3 at" "its $3"
  ui_step "$3"                             ui_click_logged "Toolbar menu entry $3"
}

# a_profile layout: a rectangle in a sketch on XY, so that Sweep (which
# needs a profile) is enabled.
a_profile() {
  ui_mark
  ui_create_sketch xy
  ui_step "rectangle tool (R)"             ui_key r
  ui_step "first corner at the origin"     ui_sketch_click 0 0
  ui_step "opposite corner"                ui_sketch_click 40 20
  ui_expect_new "Added rectangle" "$1: a rectangle, a profile for Sweep"
  ui_step "end the tool (Esc)"             ui_key Escape
  ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
  ui_expect_new "Sketch finished" "$1: the sketch finished"
}

# pin_toolbar layout: Pin to Toolbar and the Pinned group (mitcad#1001).
pin_toolbar() {
  local layout=$1
  echo "--- $layout: Pin to Toolbar"
  a_profile "$layout"
  pins_saved "" "$layout: no pins at the start"
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  row_menu "SOLID/CREATE" solid.sweep "Pin to Toolbar"
  ui_expect_new "Toolbar menu SOLID/CREATE solid.sweep: Pin to Toolbar | Change Keyboard Shortcut…" \
    "$layout: Sweep's row menu: Pin to Toolbar, Change Keyboard Shortcut…"
  ui_expect_new "Pinned to toolbar: Sweep (Solid)" "$layout: Sweep pinned"
  ui_expect_new "Ribbon group SOLID/PINNED: solid.sweep" "$layout: the Pinned group with Sweep"
  ui_sync
  ui_expect_new "Ribbon SOLID/PINNED at " "$layout: the Pinned group's caption"
  ui_expect_new "Ribbon pinned SOLID/solid.sweep at " "$layout: Sweep's button in it"
  pins_saved "solid.sweep" "$layout: saved"
  ui_mark
  ui_step "click the pinned Sweep"         ui_click_logged "Ribbon pinned SOLID/solid.sweep"
  ui_expect_new "Command Sweep started" "$layout: the pinned button starts Sweep"
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Sweep cancelled" "$layout: Sweep cancelled"

  # Loft by the keys: its row (the fifth), the Menu key, its first entry.
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  ui_expect_new "Ribbon menu SOLID/CREATE: " "$layout: the menu opened"
  ui_step "Loft's row, the Menu key"       ui_key Down Down Down Down Down Menu
  ui_expect_new "Toolbar menu SOLID/CREATE solid.loft: Pin to Toolbar | " "$layout: Loft's row menu by the Menu key"
  ui_step "Pin to Toolbar (Enter)"         ui_key Return
  ui_expect_new "Pinned to toolbar: Loft (Solid)" "$layout: Loft pinned"
  ui_expect_new "Ribbon group SOLID/PINNED: solid.sweep, solid.loft" "$layout: Loft after Sweep"
  ui_sync
  ui_expect_new "Ribbon pinned SOLID/solid.loft at " "$layout: Loft's button"
  pins_saved "solid.sweep,solid.loft" "$layout: in the order pinned"

  # Loft dragged before Sweep.
  ui_mark
  ui_step "drag Loft before Sweep"         ui_drag_logged "Ribbon pinned SOLID/solid.loft" "Ribbon pinned SOLID/solid.sweep" -12
  ui_expect_new "Pinned order (Solid): Loft, Sweep" "$layout: Loft dragged before Sweep"
  ui_expect_new "Ribbon group SOLID/PINNED: solid.loft, solid.sweep" "$layout: the group in the new order"
  pins_saved "solid.loft,solid.sweep" "$layout: the new order saved"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Command Loft started" && ui_fail "$layout: the drag started Loft"
  echo "ok   $layout: the drag started no command"

  # Sweep unpinned from its button's right-click.
  ui_sync
  ui_mark
  ui_step "right-click the pinned Sweep"   ui_click_logged "Ribbon pinned SOLID/solid.sweep" 3
  ui_expect_new "Toolbar menu SOLID/PINNED solid.sweep: Unpin from Toolbar | Change Keyboard Shortcut…" \
    "$layout: the pinned button's menu"
  ui_expect_new "Toolbar menu entry Unpin from Toolbar at" "its Unpin from Toolbar"
  ui_step "Unpin from Toolbar"             ui_click_logged "Toolbar menu entry Unpin from Toolbar"
  ui_expect_new "Unpinned: Sweep (Solid)" "$layout: Sweep unpinned"
  ui_expect_new "Ribbon group SOLID/PINNED: solid.loft" "$layout: Loft left"
  pins_saved "solid.loft" "$layout: Sweep no longer saved"

  # Loft unpinned from its row in the CREATE menu: the group is gone.
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  row_menu "SOLID/CREATE" solid.loft "Unpin from Toolbar"
  ui_expect_new "Unpinned: Loft (Solid)" "$layout: Loft unpinned from its row"
  ui_expect_new "Ribbon group SOLID/PINNED gone" "$layout: the Pinned group is gone"
  pins_saved "" "$layout: no pins saved"
  ui_sync
  [ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c "Ribbon group SOLID/PINNED: ")" = 0 ] ||
    ui_fail "$layout: the Pinned group came back"

  [ "$layout" = Docked ] || return 0
  echo "--- $layout: Change Keyboard Shortcut…"
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  row_menu "SOLID/CREATE" solid.sweep "Change Keyboard Shortcut…"
  ui_expect_new "Keyboard Shortcuts opened" "$layout: Keyboard Shortcuts opened"
  ui_expect_new "Keyboard Shortcuts at Sweep" "$layout: at Sweep's row"
  ui_focus_dialog '^Keyboard Shortcuts$'
  ui_step "close it (Esc)"                 ui_key Escape
  ui_focus_main
}

# narrow_window: the Floating layout's groups in a narrow window, the ones
# at the right as their names only (Ribbon::fitGroups), all within it.
narrow_window() {
  echo "--- A narrow window"
  ui_step "a narrow window"                xdotool windowsize "$UI_WINDOW" 900 800
  ui_mark
  ui_create_sketch
  ui_expect_new "Ribbon SKETCH/FINISH SKETCH at" "the groups logged again"
  within_window "Floating, narrow"
  ui_mark
  ui_step "open the CREATE menu"           ui_click_logged "Ribbon SKETCH/CREATE"
  ui_expect_new "Ribbon menu SKETCH/CREATE: $CREATE" "the CREATE menu in a narrow window"
  ui_step "close it (Esc)"                 ui_key Escape
}

ui_start_display
ui_start_app

echo "--- The SOLID CREATE menu"
ui_mark
ui_step "open the SOLID CREATE menu"       ui_click_logged "Ribbon SOLID/CREATE"
ui_expect_new "Ribbon menu SOLID/CREATE: " "the SOLID CREATE menu opened"
case "$(last_line "Ribbon menu SOLID/CREATE: ")" in
"$SOLID_CREATE]" | "$SOLID_CREATE | "*"]") echo "ok   the SOLID CREATE menu in its sections and submenus" ;;
*) ui_fail "the SOLID CREATE menu: '$(last_line "Ribbon menu SOLID/CREATE: ")'" ;;
esac
# The keyboard: Up from the top is the last entry (Machine Elements), its
# submenu's first entry Gear.
ui_mark
ui_step "Machine Elements > Gear"          ui_key Up Right Return
ui_expect_new "Ribbon menu SOLID/CREATE/Machine Elements: Gear | Gear Pair | Pulley | Belt Drive | Bearing" "the Machine Elements submenu opened"
ui_expect_new "Command Gear started" "Gear from the Machine Elements submenu"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Gear cancelled" "Gear cancelled"
# None of their commands is pinned: no split buttons, the buttons as before.
grep -qF "Ribbon split SOLID/CREATE/" "$UI_LOG" && ui_fail "a split button for a SOLID CREATE submenu"
echo "ok   no split buttons for the Pattern and Machine Elements submenus"
for id in sketch.create solid.extrude solid.revolve solid.hole; do
  ui_expect_log "Ribbon button $id at " "the $id button"
done
for id in solid.rectangular_pattern solid.mirror solid.gear solid.pulley solid.bearing solid.plastic; do
  grep -qF "Ribbon button $id at " "$UI_LOG" && ui_fail "a button for $id"
done
echo "ok   no buttons for the patterns, Mirror, Plastic Feature and the machine elements"

pin_toolbar "Docked"
check_layout "Docked"
ui_stop_app
ui_start_app --chrome=floating
pin_toolbar "Floating"
check_layout "Floating"
narrow_window

ui_finish "UI sketch create menu test"
