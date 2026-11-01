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
#   - Pin to Toolbar (mitcad#1001), in both layouts in a 1600 x 900 window,
#     with a rectangle sketched first (Sweep needs a profile): Sweep pinned
#     from the ⋮ button of its row in the SOLID CREATE menu is a button in
#     the CREATE group, after its own buttons and before MODIFY
#     (mitcad#1084), and it starts Sweep; the window narrowed until Sweep's
#     button folds: CREATE's own buttons stay, Sweep starts from the CREATE
#     menu, unpinned there the row is as before (mitcad#1111); Loft pinned
#     by the Menu key on its row, after Sweep; Loft dragged before Sweep;
#     Draft pinned from its row in the MODIFY menu is a button in MODIFY;
#     Floating also folds Draft in a narrow window; Sweep unpinned
#     from its button's right-click, Draft and Loft from their rows' menus,
#     no pinned buttons left; never a separate Pinned group; the preference
#     follows each step (toolbarPins/SOLID). Docked also Change Keyboard
#     Shortcut…: Keyboard Shortcuts at Sweep.
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

# x_of "prefix": the x of the last place logged as "<prefix> at x,y" ("" none).
x_of() {
  last_line "$1 at " | cut -d, -f1
}

# pinned_in_group "TAB/GROUP" id "next group" own...: the pinned button of
# the command is in its own group (mitcad#1084): logged with the group's
# name, right of the group's own buttons and left of the next group's name.
pinned_in_group() {
  local group=$1 id=$2 next=$3 x nx own ox
  shift 3
  x=$(x_of "Ribbon pinned $group/$id")
  nx=$(x_of "Ribbon $next")
  [ -n "$x" ] && [ -n "$nx" ] || ui_fail "$id in $group: not logged (at x '$x', $next at '$nx')"
  [ "$x" -lt "$nx" ] || ui_fail "$id in $group: at x $x, not before $next at $nx"
  for own in "$@"; do
    ox=$(x_of "Ribbon button $own")
    [ -n "$ox" ] && [ "$ox" -lt "$x" ] || ui_fail "$id in $group: at x $x, not after the group's $own (at '$ox')"
  done
  echo "ok   $id's button in $group, after the group's own buttons"
}

# no_pinned_group layout: no separate Pinned group was made or logged.
no_pinned_group() {
  grep -qF "/PINNED" "$UI_LOG" && ui_fail "$1: a Pinned group: $(grep -F "/PINNED" "$UI_LOG" | tail -1)"
  echo "ok   $1: no Pinned group"
}

# last_layout: the log from the places logged last (their first line, the
# SOLID tab's) on.
last_layout() {
  local start
  start=$(grep -nF "Ribbon tab SOLID at " "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG"
}

# pins_fold layout: in a narrow window the pinned Draft folds before its
# MODIFY group does (mitcad#1111), whether MODIFY's own buttons then fit or
# fold to its name; wide again, Draft is back.
pins_fold() {
  local layout=$1
  ui_mark
  ui_step "a narrow window"                xdotool windowsize "$UI_WINDOW" 900 800
  ui_expect_new "Ribbon SOLID/MODIFY at " "$layout, narrow: the groups logged again"
  ui_expect_new "Ribbon pinned folded SOLID/MODIFY: solid.draft" "$layout, narrow: the pinned Draft folded"
  ui_sync
  sleep 1
  ui_sync
  last_layout | grep -qF "Ribbon SOLID/MODIFY at " || ui_fail "$layout, narrow: no MODIFY name"
  last_layout | grep -qF "Ribbon pinned SOLID/MODIFY/solid.draft at " &&
    ui_fail "$layout, narrow: the pinned Draft not folded"
  echo "ok   $layout, narrow: the pinned Draft folded, MODIFY's name in sight"
  ui_mark
  ui_step "a wide window again"            xdotool windowsize "$UI_WINDOW" 1600 900
  ui_expect_new "Ribbon pinned SOLID/MODIFY/solid.draft at " "$layout, wide: Draft back in MODIFY"
  ui_expect_new "Ribbon pinned folded SOLID/MODIFY: none" "$layout, wide: no folded pins in MODIFY"
  ui_sync
}

# create_row layout description: CREATE's own buttons in the places logged
# last, Sweep's pinned button not.
create_row() {
  local id
  for id in sketch.create solid.extrude solid.revolve solid.hole; do
    last_layout | grep -qF "Ribbon button $id at " || ui_fail "$1: CREATE's $id not in sight"
  done
  last_layout | grep -qF "Ribbon pinned SOLID/CREATE/solid.sweep at " && ui_fail "$1: Sweep's pinned button in sight"
  echo "ok   $1"
}

# pins_fold_create layout: with Sweep pinned, the window narrowed step by
# step until Sweep's button folds (mitcad#1111): CREATE's own buttons stay
# (Create Sketch, Extrude, Revolve, Hole), Sweep starts from the CREATE
# menu; unpinned from its row there, no folded pins and the row as before;
# wide again, Sweep pinned again by the keys.
pins_fold_create() {
  local layout=$1 width=$WIDTH
  ui_mark
  while ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Ribbon pinned folded SOLID/CREATE: solid.sweep"; do
    width=$((width - 12))
    [ "$width" -ge 500 ] || ui_fail "$layout: Sweep's button not folded in a window down to 500 px"
    xdotool windowsize "$UI_WINDOW" "$width" 900
    ui_sync
    ui_sync
  done
  echo "ok   $layout: Sweep's button folded in a $width px window"
  sleep 1
  ui_sync
  # The fold may have been read a step late, past where CREATE folds too:
  # back in small steps until its own buttons show (Sweep's needs more).
  while ! last_layout | grep -qF "Ribbon button sketch.create at " && [ "$width" -lt "$WIDTH" ]; do
    width=$((width + 4))
    xdotool windowsize "$UI_WINDOW" "$width" 900
    ui_sync
    sleep 0.3
    ui_sync
  done
  [ "$(last_line "Ribbon pinned folded SOLID/CREATE: ")" = "solid.sweep" ] ||
    ui_fail "$layout: Sweep's button not folded in a $width px window"
  create_row "$layout, $width px: CREATE's own buttons in sight, Sweep's button folded"
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  ui_expect_new "Ribbon menu SOLID/CREATE: " "$layout: the menu opened with Sweep folded"
  case "$(last_line "Ribbon menu SOLID/CREATE: ")" in
  *" | Revolve | Sweep | Loft | "*) echo "ok   $layout: the folded Sweep in the CREATE menu" ;;
  *) ui_fail "$layout: no Sweep in the CREATE menu: '$(last_line "Ribbon menu SOLID/CREATE: ")'" ;;
  esac
  ui_step "Sweep's row (Enter)"            ui_key Down Down Down Down Return
  ui_expect_new "Command Sweep started" "$layout: the folded Sweep starts from the CREATE menu"
  ui_mark
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Sweep cancelled" "$layout: Sweep cancelled"
  # Unpinned from its row: no folded pins, the row as before the pin.
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  ui_expect_new "Ribbon menu SOLID/CREATE: " "$layout: the menu opened"
  ui_step "Sweep's row, the Menu key"      ui_key Down Down Down Down Menu
  ui_expect_new "Toolbar menu SOLID/CREATE solid.sweep: Unpin from Toolbar | " "$layout: Sweep's row menu: Unpin from Toolbar"
  ui_step "Unpin from Toolbar (Enter)"     ui_key Return
  ui_expect_new "Unpinned: Sweep (Solid)" "$layout: the folded Sweep unpinned"
  ui_expect_new "Ribbon pinned folded SOLID/CREATE: none" "$layout: no folded pins left"
  sleep 0.5
  ui_sync
  create_row "$layout, unpinned: CREATE's row as before the pin"
  pins_saved "" "$layout: Sweep no longer saved"
  # Wide again, Sweep pinned again for the steps below.
  ui_step "a wide window again"            xdotool windowsize "$UI_WINDOW" "$WIDTH" 900
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  ui_expect_new "Ribbon menu SOLID/CREATE: " "$layout: the menu opened"
  ui_step "Sweep's row, the Menu key"      ui_key Down Down Down Down Menu
  ui_expect_new "Toolbar menu SOLID/CREATE solid.sweep: Pin to Toolbar | " "$layout: Sweep's row menu: Pin to Toolbar"
  ui_step "Pin to Toolbar (Enter)"         ui_key Return
  ui_expect_new "Pinned to toolbar: Sweep (Solid)" "$layout: Sweep pinned again"
  ui_sync
  ui_expect_new "Ribbon pinned SOLID/CREATE/solid.sweep at " "$layout: Sweep's button in CREATE again"
  pins_saved "solid.sweep" "$layout: saved again"
}

# pin_toolbar layout: Pin to Toolbar (mitcad#1001), each pinned command a
# button in its own group (mitcad#1084).
pin_toolbar() {
  local layout=$1
  echo "--- $layout: Pin to Toolbar"
  # Wide enough for the Create and Modify groups' buttons with the pins.
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 900
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  a_profile "$layout"
  pins_saved "" "$layout: no pins at the start"
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  row_menu "SOLID/CREATE" solid.sweep "Pin to Toolbar"
  ui_expect_new "Toolbar menu SOLID/CREATE solid.sweep: Pin to Toolbar | Change Keyboard Shortcut…" \
    "$layout: Sweep's row menu: Pin to Toolbar, Change Keyboard Shortcut…"
  ui_expect_new "Pinned to toolbar: Sweep (Solid)" "$layout: Sweep pinned"
  ui_expect_new "Ribbon group SOLID/CREATE pinned: solid.sweep" "$layout: Sweep in the CREATE group"
  ui_sync
  ui_expect_new "Ribbon pinned SOLID/CREATE/solid.sweep at " "$layout: Sweep's button in CREATE"
  pinned_in_group SOLID/CREATE solid.sweep SOLID/MODIFY sketch.create solid.extrude solid.revolve solid.hole
  no_pinned_group "$layout"
  pins_saved "solid.sweep" "$layout: saved"
  ui_mark
  ui_step "click the pinned Sweep"         ui_click_logged "Ribbon pinned SOLID/CREATE/solid.sweep"
  ui_expect_new "Command Sweep started" "$layout: the pinned button starts Sweep"
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Sweep cancelled" "$layout: Sweep cancelled"
  pins_fold_create "$layout"

  # Loft by the keys: its row (the fifth), the Menu key, its first entry.
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  ui_expect_new "Ribbon menu SOLID/CREATE: " "$layout: the menu opened"
  ui_step "Loft's row, the Menu key"       ui_key Down Down Down Down Down Menu
  ui_expect_new "Toolbar menu SOLID/CREATE solid.loft: Pin to Toolbar | " "$layout: Loft's row menu by the Menu key"
  ui_step "Pin to Toolbar (Enter)"         ui_key Return
  ui_expect_new "Pinned to toolbar: Loft (Solid)" "$layout: Loft pinned"
  ui_expect_new "Ribbon group SOLID/CREATE pinned: solid.sweep, solid.loft" "$layout: Loft after Sweep in CREATE"
  ui_sync
  ui_expect_new "Ribbon pinned SOLID/CREATE/solid.loft at " "$layout: Loft's button"
  pinned_in_group SOLID/CREATE solid.loft SOLID/MODIFY solid.hole
  pins_saved "solid.sweep,solid.loft" "$layout: in the order pinned"

  # Loft dragged before Sweep, among CREATE's pinned buttons.
  ui_mark
  ui_step "drag Loft before Sweep"         ui_drag_logged "Ribbon pinned SOLID/CREATE/solid.loft" "Ribbon pinned SOLID/CREATE/solid.sweep" -12
  ui_expect_new "Pinned order SOLID/CREATE: Loft, Sweep" "$layout: Loft dragged before Sweep"
  ui_expect_new "Ribbon group SOLID/CREATE pinned: solid.loft, solid.sweep" "$layout: CREATE's pins in the new order"
  pins_saved "solid.loft,solid.sweep" "$layout: the new order saved"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Command Loft started" && ui_fail "$layout: the drag started Loft"
  echo "ok   $layout: the drag started no command"

  # Draft, a Modify command, from the ⋮ of its row in the MODIFY menu: its
  # button in MODIFY, CREATE's pins as they were.
  ui_mark
  ui_step "open the SOLID MODIFY menu"     ui_click_logged "Ribbon SOLID/MODIFY"
  row_menu "SOLID/MODIFY" solid.draft "Pin to Toolbar"
  ui_expect_new "Pinned to toolbar: Draft (Solid)" "$layout: Draft pinned"
  ui_expect_new "Ribbon group SOLID/MODIFY pinned: solid.draft" "$layout: Draft in the MODIFY group"
  ui_sync
  ui_expect_new "Ribbon pinned SOLID/MODIFY/solid.draft at " "$layout: Draft's button in MODIFY"
  pinned_in_group SOLID/MODIFY solid.draft "SOLID/SHEET METAL" solid.shell solid.fillet
  [ "$(x_of "Ribbon SOLID/CREATE")" -lt "$(x_of "Ribbon pinned SOLID/MODIFY/solid.draft")" ] ||
    ui_fail "$layout: Draft's button not right of the CREATE group"
  no_pinned_group "$layout"
  pins_saved "solid.loft,solid.sweep,solid.draft" "$layout: Draft saved after them"
  [ "$layout" = Floating ] && pins_fold "$layout"

  # Sweep unpinned from its button's right-click.
  ui_sync
  ui_mark
  ui_step "right-click the pinned Sweep"   ui_click_logged "Ribbon pinned SOLID/CREATE/solid.sweep" 3
  ui_expect_new "Toolbar menu SOLID/CREATE solid.sweep: Unpin from Toolbar | Change Keyboard Shortcut…" \
    "$layout: the pinned button's menu"
  ui_expect_new "Toolbar menu entry Unpin from Toolbar at" "its Unpin from Toolbar"
  ui_step "Unpin from Toolbar"             ui_click_logged "Toolbar menu entry Unpin from Toolbar"
  ui_expect_new "Unpinned: Sweep (Solid)" "$layout: Sweep unpinned"
  ui_expect_new "Ribbon group SOLID/CREATE pinned: solid.loft" "$layout: Loft left in CREATE"
  pins_saved "solid.loft,solid.draft" "$layout: Sweep no longer saved"

  # Draft unpinned from its row in the MODIFY menu: MODIFY without pins.
  ui_mark
  ui_step "open the SOLID MODIFY menu"     ui_click_logged "Ribbon SOLID/MODIFY"
  row_menu "SOLID/MODIFY" solid.draft "Unpin from Toolbar"
  ui_expect_new "Unpinned: Draft (Solid)" "$layout: Draft unpinned from its row"
  ui_expect_new "Ribbon group SOLID/MODIFY pinned: none" "$layout: MODIFY without pins"
  pins_saved "solid.loft" "$layout: Draft no longer saved"

  # Loft unpinned from its row in the CREATE menu: no pins left.
  ui_mark
  ui_step "open the SOLID CREATE menu"     ui_click_logged "Ribbon SOLID/CREATE"
  row_menu "SOLID/CREATE" solid.loft "Unpin from Toolbar"
  ui_expect_new "Unpinned: Loft (Solid)" "$layout: Loft unpinned from its row"
  ui_expect_new "Ribbon group SOLID/CREATE pinned: none" "$layout: CREATE without pins"
  pins_saved "" "$layout: no pins saved"
  ui_sync
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Ribbon pinned SOLID/" && ui_fail "$layout: a pinned button left"
  echo "ok   $layout: no pinned buttons left"
  no_pinned_group "$layout"

  if [ "$layout" = Docked ]; then
    echo "--- $layout: Change Keyboard Shortcut…"
    ui_mark
    ui_step "open the SOLID CREATE menu"   ui_click_logged "Ribbon SOLID/CREATE"
    row_menu "SOLID/CREATE" solid.sweep "Change Keyboard Shortcut…"
    ui_expect_new "Keyboard Shortcuts opened" "$layout: Keyboard Shortcuts opened"
    ui_expect_new "Keyboard Shortcuts at Sweep" "$layout: at Sweep's row"
    ui_focus_dialog '^Keyboard Shortcuts$'
    ui_step "close it (Esc)"               ui_key Escape
    ui_focus_main
  fi
  # The tests' window again for the rest.
  ui_step "the tests' window"              xdotool windowsize "$UI_WINDOW" 1280 800
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
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
