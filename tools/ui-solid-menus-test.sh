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
# check-all sources: app/resources/layout/default.json app/framework/CommandLayout.cpp app/commands/InspectCommands.cpp
# The modelling tabs' menus in their sections (mitcad#1778, mitcad#1975),
# through the real UI, in the Docked and the Floating layout: SOLID's
# MODIFY, ASSEMBLE, CONSTRUCT, INSPECT and INSERT, SURFACE's CREATE,
# MODIFY and CONSTRUCT, SHEET METAL's CREATE, MODIFY and RULES and
# MANAGE's MATERIALS, RULES and DESIGN as they open, every entry in its
# place and a separator between two sections (app/COMMANDS.md,
# "Sections"), so that a command registered out of its section's place
# fails here; and each group's own buttons in the menu's order, as the
# ribbon logs them when it makes the group ("Ribbon group SOLID/MODIFY
# buttons: ..."). Surface and Sheet Metal show SOLID's CONSTRUCT group too
# (a second place, mitcad#1975).
# The Windows counterpart: tools/ui-windows-solid-menus-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-solid-menus-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

MODIFY="Press Pull | Fillet | Chamfer | Shell | Draft"
MODIFY="$MODIFY | - | Offset Face | Move Face | Replace Face | Fill Cavity | Split Face | Delete Face"
MODIFY="$MODIFY | - | Combine | Split Body | Panelize | Scale | Convert Mesh"
MODIFY="$MODIFY | Mesh > [Mesh Section | Repair Mesh | Decimate Mesh | Remove Region | Segment Mesh | Fit Surface]"
MODIFY="$MODIFY | - | Move/Copy | Align"
MODIFY="$MODIFY | - | Joinery > [Dogbone | Mortise and Tenon]"
MODIFY="$MODIFY | - | Material | Appearance"
ASSEMBLE="New Component"
ASSEMBLE="$ASSEMBLE | - | Joint | As-Built Joint | Joint Origin | Rigid Group | Constraint | Constraint Set"
ASSEMBLE="$ASSEMBLE | - | Drive Joint | Motion Link | Contact Set | - | Let It Hang | - | Animate Joint | Motion Study… | Explode…"
CONSTRUCT="Offset Plane | Plane at Angle | Tangent Plane | Midplane | Plane Through Two Edges"
CONSTRUCT="$CONSTRUCT | Plane Through Three Points | Plane Tangent to Face at Point | Plane Perpendicular to Face at Point | Plane Along Path"
CONSTRUCT="$CONSTRUCT | Plane Through Edge and Point | Plane Normal to Path at Point | Offset Plane Through Point"
CONSTRUCT="$CONSTRUCT | - | Axis Through Cylinder/Cone/Torus | Axis Perpendicular at Point"
CONSTRUCT="$CONSTRUCT | Axis Through Two Planes | Axis Through Two Points | Axis Through Edge"
CONSTRUCT="$CONSTRUCT | Axis Perpendicular to Face at Point"
CONSTRUCT="$CONSTRUCT | - | Point at Vertex | Point Through Two Edges | Point Through Three Planes"
CONSTRUCT="$CONSTRUCT | Point at Center of Circle/Sphere/Torus | Point at Edge and Plane | Point Along Path"
CONSTRUCT="$CONSTRUCT | - | Coordinate System"
INSPECT="Measure | Interference | Section Analysis | Print Analysis | Draft Analysis | Point Cloud Deviation | Physical Properties | Center of Mass | Add Comment"
INSERT="Insert Component | Insert from Library"
INSERT="$INSERT | - | Insert Mesh | Insert DXF | Insert SVG | Insert Points | From an Open Design… | Component from Web… | Import…"
INSERT="$INSERT | Image Reference | Point Cloud | Decal"
SURFACE_CREATE="Create Sketch | - | Surface Extrude | Surface Revolve | Surface Sweep | Surface Loft | Surface Helix"
SURFACE_CREATE="$SURFACE_CREATE | - | Patch | Ruled Surface | Boundary Surface | Blend Surface | - | Offset Surface"
SURFACE_MODIFY="Trim | Untrim | Extend | - | Stitch | Unstitch | Reverse Normal | - | Match Surface | Fair Surface"
SURFACE_MODIFY="$SURFACE_MODIFY | - | Thicken | Boundary Fill"
SHEET_CREATE="Create Sketch | - | Face | Flange | Hem | Convert to Sheet Metal | - | Bend | Fold"
SHEET_MODIFY="Sheet Cut | Perforate | - | Corner Relief | Corner Round | Corner Chamfer"
SHEET_MODIFY="$SHEET_MODIFY | - | Unfold | Refold | Flat Pattern | Show Flat Pattern"
SHEET_MODIFY="$SHEET_MODIFY | - | Export Flat Pattern… | Create Drawing of Flat Pattern… | Export Bend Table…"
RULES="Sheet Rules | Edit Sheet Rules"
MANAGE_RULES="$RULES | - | Plastic Rules"
DESIGN="Recompute All | Units… | - | Design Properties…"
MATERIALS="Edit Materials… | Edit Appearances…"

# last_line "prefix": the last log line containing the prefix, without the
# prefix and what is before it.
last_line() {
  grep -F -- "$1" "$UI_LOG" | tail -1 | awk -v p="$1" '{ print substr($0, index($0, p) + length(p)) }'
}

# show_tab layout TAB: the tab shown, unless it is already (the last one
# shown since the app started).
show_tab() {
  local layout=$1 tab=$2 shown
  # SOLID at the start, before a tab was chosen.
  shown=$(last_line "Ribbon tab shown: ")
  [ "${shown:-SOLID}" = "$tab" ] && return
  ui_mark
  ui_step "the $tab tab"                   ui_click_tab "$tab"
  ui_expect_new "Ribbon tab shown: $tab" "$layout: the $tab tab shown"
}

# check_menu layout TAB/GROUP "entries": the group's menu as it opens is
# the entries ("-" a separator, "Name > [...]" a submenu), exactly.
check_menu() {
  local layout=$1 place=$2 expected=$3 menu
  show_tab "$layout" "${place%%/*}"
  ui_mark
  ui_step "open the $place menu"           ui_click_logged "Ribbon $place"
  ui_expect_new "Ribbon menu $place: " "$layout: the $place menu opened"
  ui_step "close it (Esc)"                 ui_key Escape
  menu=$(last_line "Ribbon menu $place: ")
  [ "$menu" = "$expected" ] || ui_fail "$layout: the $place menu is '$menu', not '$expected'"
  echo "ok   $layout: the $place menu in its sections"
}

# check_buttons layout TAB/GROUP "ids": the group's own buttons, in order.
check_buttons() {
  local layout=$1 place=$2 expected=$3 buttons
  ui_expect_log "Ribbon group $place buttons: " "$layout: the $place group's buttons logged"
  buttons=$(last_line "Ribbon group $place buttons: ")
  [ "$buttons" = "$expected" ] || ui_fail "$layout: the $place buttons are '$buttons', not '$expected'"
  echo "ok   $layout: the $place buttons $expected"
}

check_layout() {
  local layout=$1
  echo "=== $layout"
  check_buttons "$layout" SOLID/MODIFY "solid.press_pull, solid.fillet, solid.chamfer, solid.shell"
  check_buttons "$layout" SOLID/ASSEMBLE "solid.new_component, assemble.joint, assemble.constraint_one"
  check_buttons "$layout" SOLID/CONSTRUCT "construct.offset_plane, construct.plane_angle, construct.midplane"
  check_buttons "$layout" SOLID/INSPECT "inspect.measure, inspect.section"
  check_buttons "$layout" SOLID/INSERT "insert.component, insert.mesh, insert.dxf"
  check_buttons "$layout" SURFACE/CREATE "sketch.create, surface.extrude, surface.patch"
  check_buttons "$layout" SURFACE/MODIFY "surface.trim, surface.thicken"
  check_buttons "$layout" SURFACE/CONSTRUCT "construct.offset_plane, construct.plane_angle, construct.midplane"
  check_buttons "$layout" "SHEET METAL/CREATE" "sketch.create, sheet.face, sheet.flange, sheet.bend"
  check_buttons "$layout" "SHEET METAL/MODIFY" "sheet.unfold, sheet.flat_pattern"
  check_buttons "$layout" "SHEET METAL/RULES" "sheet.rules"
  check_buttons "$layout" MANAGE/PARAMETERS "solid.parameters, manage.favorite_parameters"
  check_buttons "$layout" MANAGE/CONFIGURATIONS "solid.configurations"
  check_buttons "$layout" MANAGE/MATERIALS "solid.materials, solid.appearances"
  check_buttons "$layout" MANAGE/RULES "sheet.rules, manage.plastic_rules"
  check_buttons "$layout" MANAGE/DESIGN "manage.recompute_all, manage.units, file.design_properties"
  check_menu "$layout" SOLID/MODIFY "$MODIFY"
  check_menu "$layout" SOLID/ASSEMBLE "$ASSEMBLE"
  check_menu "$layout" SOLID/CONSTRUCT "$CONSTRUCT"
  check_menu "$layout" SOLID/INSPECT "$INSPECT"
  check_menu "$layout" SOLID/INSERT "$INSERT"
  check_menu "$layout" SURFACE/CREATE "$SURFACE_CREATE"
  check_menu "$layout" SURFACE/MODIFY "$SURFACE_MODIFY"
  check_menu "$layout" SURFACE/CONSTRUCT "$CONSTRUCT"
  check_menu "$layout" "SHEET METAL/CREATE" "$SHEET_CREATE"
  check_menu "$layout" "SHEET METAL/MODIFY" "$SHEET_MODIFY"
  check_menu "$layout" "SHEET METAL/RULES" "$RULES"
  check_menu "$layout" MANAGE/MATERIALS "$MATERIALS"
  check_menu "$layout" MANAGE/RULES "$MANAGE_RULES"
  check_menu "$layout" MANAGE/DESIGN "$DESIGN"
}

ui_start_display
ui_start_app
check_layout "Docked"
ui_stop_app
ui_start_app --chrome=floating
check_layout "Floating"

ui_finish "UI solid menus test"
