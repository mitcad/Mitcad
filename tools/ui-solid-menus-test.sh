#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/Command.hpp
# check-all sources: app/commands/ModifyCommands.cpp app/commands/SheetCommands.cpp app/MainWindowSheet.cpp
# check-all sources: app/commands/SurfaceCommands.cpp app/commands/ConstructCommands.cpp
# check-all sources: app/commands/JointCommands.cpp app/commands/PrimitiveCommands.cpp
# check-all sources: app/commands/FilletCommands.cpp app/commands/FaceCommands.cpp app/commands/MoveCommands.cpp
# check-all sources: app/commands/MaterialCommands.cpp app/commands/DogboneCommands.cpp
# check-all sources: app/commands/MortiseTenonCommands.cpp app/commands/ConvertMeshCommands.cpp
# check-all sources: app/MainWindow.cpp app/files/MainWindowFiles.cpp app/files/MainWindowLibraries.cpp
# The SOLID tab's menus in their sections (mitcad#1778), through the real
# UI, in the Docked and the Floating layout: MODIFY, SHEET METAL, SURFACE,
# ASSEMBLE and CONSTRUCT as they open, every entry in its place and a
# separator between two sections (app/COMMANDS.md, "Sections"), so that a
# command registered out of its section's place fails here; and each
# group's own buttons in the menu's order, as the ribbon logs them when it
# makes the group ("Ribbon group SOLID/MODIFY buttons: ...").
# The Windows counterpart: tools/ui-windows-solid-menus-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-solid-menus-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

MODIFY="Press Pull | Fillet | Chamfer | Shell | Draft"
MODIFY="$MODIFY | - | Offset Face | Move Face | Replace Face | Split Face | Delete Face"
MODIFY="$MODIFY | - | Combine | Split Body | Scale | Convert Mesh"
MODIFY="$MODIFY | - | Move/Copy | Align"
MODIFY="$MODIFY | - | Dogbone | Mortise and Tenon"
MODIFY="$MODIFY | - | Material | Appearance | Edit Appearances…"
MODIFY="$MODIFY | - | Edit Parameters… | Configurations…"
SHEET="Sheet Rules | Edit Sheet Rules"
SHEET="$SHEET | - | Face | Flange"
SHEET="$SHEET | - | Bend | Fold | Corner Relief"
SHEET="$SHEET | - | Corner Round | Corner Chamfer | Sheet Cut"
SHEET="$SHEET | - | Unfold | Refold | Flat Pattern | Show Flat Pattern"
SURFACE="Surface Extrude | Surface Revolve | Surface Sweep | Surface Loft | Surface Helix"
SURFACE="$SURFACE | - | Patch | Ruled Surface | Boundary Surface | Blend Surface"
SURFACE="$SURFACE | - | Extend | Trim | Untrim | Offset Surface"
SURFACE="$SURFACE | - | Match Surface | Fair Surface | Reverse Normal"
SURFACE="$SURFACE | - | Stitch | Unstitch | Thicken"
ASSEMBLE="New Component"
ASSEMBLE="$ASSEMBLE | - | Joint | As-Built Joint | Joint Origin | Rigid Group | Assembly Constraint"
ASSEMBLE="$ASSEMBLE | - | Drive Joint | Motion Link | Contact Set | Animate Joint | Motion Study…"
ASSEMBLE="$ASSEMBLE | - | Insert Component | Insert from Library"
CONSTRUCT="Offset Plane | Plane at Angle | Tangent Plane | Midplane | Plane Through Two Edges"
CONSTRUCT="$CONSTRUCT | Plane Through Three Points | Plane Tangent to Face at Point | Plane Along Path"
CONSTRUCT="$CONSTRUCT | Plane Through Edge and Point | Plane Normal to Path at Point | Offset Plane Through Point"
CONSTRUCT="$CONSTRUCT | - | Axis Through Cylinder/Cone/Torus | Axis Perpendicular at Point"
CONSTRUCT="$CONSTRUCT | Axis Through Two Planes | Axis Through Two Points | Axis Through Edge"
CONSTRUCT="$CONSTRUCT | Axis Perpendicular to Face at Point"
CONSTRUCT="$CONSTRUCT | - | Point at Vertex | Point Through Two Edges | Point Through Three Planes"
CONSTRUCT="$CONSTRUCT | Point at Center of Circle/Sphere/Torus | Point at Edge and Plane | Point Along Path"
CONSTRUCT="$CONSTRUCT | - | Coordinate System"

# last_line "prefix": the last log line containing the prefix, without the
# prefix and what is before it.
last_line() {
  grep -F -- "$1" "$UI_LOG" | tail -1 | awk -v p="$1" '{ print substr($0, index($0, p) + length(p)) }'
}

# check_menu layout GROUP "entries": the group's menu as it opens is the
# entries ("-" a separator), exactly.
check_menu() {
  local layout=$1 group=$2 expected=$3 menu
  ui_mark
  ui_step "open the $group menu"           ui_click_logged "Ribbon SOLID/$group"
  ui_expect_new "Ribbon menu SOLID/$group: " "$layout: the $group menu opened"
  ui_step "close it (Esc)"                 ui_key Escape
  menu=$(last_line "Ribbon menu SOLID/$group: ")
  [ "$menu" = "$expected" ] || ui_fail "$layout: the $group menu is '$menu', not '$expected'"
  echo "ok   $layout: the $group menu in its sections"
}

# check_buttons layout GROUP "ids": the group's own buttons, in order.
check_buttons() {
  local layout=$1 group=$2 expected=$3 buttons
  ui_expect_log "Ribbon group SOLID/$group buttons: " "$layout: the $group group's buttons logged"
  buttons=$(last_line "Ribbon group SOLID/$group buttons: ")
  [ "$buttons" = "$expected" ] || ui_fail "$layout: the $group buttons are '$buttons', not '$expected'"
  echo "ok   $layout: the $group buttons $expected"
}

check_layout() {
  local layout=$1
  echo "=== $layout"
  check_buttons "$layout" MODIFY "solid.press_pull, solid.fillet, solid.chamfer, solid.shell, solid.combine"
  check_buttons "$layout" "SHEET METAL" "sheet.face, sheet.flange, sheet.bend, sheet.unfold, sheet.flat_pattern"
  check_buttons "$layout" SURFACE "surface.extrude, surface.patch, surface.trim, surface.thicken"
  check_buttons "$layout" ASSEMBLE "solid.new_component, assemble.joint, assemble.constraint, insert.component"
  check_buttons "$layout" CONSTRUCT "construct.offset_plane, construct.plane_angle, construct.midplane"
  check_menu "$layout" MODIFY "$MODIFY"
  check_menu "$layout" "SHEET METAL" "$SHEET"
  check_menu "$layout" SURFACE "$SURFACE"
  check_menu "$layout" ASSEMBLE "$ASSEMBLE"
  check_menu "$layout" CONSTRUCT "$CONSTRUCT"
}

ui_start_display
ui_start_app
check_layout "Docked"
ui_stop_app
ui_start_app --chrome=floating
check_layout "Floating"

ui_finish "UI solid menus test"
