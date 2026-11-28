# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Command.hpp app/MainWindow.cpp
# check-all sources: app/commands/ModifyCommands.cpp app/commands/SheetCommands.cpp app/MainWindowSheet.cpp
# check-all sources: app/commands/SurfaceCommands.cpp app/commands/ConstructCommands.cpp
# check-all sources: app/commands/JointCommands.cpp app/commands/PrimitiveCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/files/MainWindowLibraries.cpp
# check-all sources: app/resources/layout/default.json app/framework/CommandLayout.cpp app/commands/InspectCommands.cpp
# The modelling tabs' menus in their sections (mitcad#1778, mitcad#1975)
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# in the Docked and the Floating layout, as tools/ui-solid-menus-test.sh
# does on Linux: SOLID's MODIFY, ASSEMBLE, CONSTRUCT, INSPECT and INSERT,
# SURFACE's CREATE, MODIFY and CONSTRUCT, SHEET PARTS's CREATE, MODIFY and
# RULES and MANAGE's MATERIALS, RULES and DESIGN as they open, every entry
# in its place and a separator ("-") between two sections
# (app/COMMANDS.md, "Sections"), so that a command registered out of its
# section's place fails here; and each group's own buttons in the menu's
# order, as the ribbon logs them when it makes the group ("Ribbon group
# SOLID/MODIFY buttons: ...").
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-solid-menus-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-solid-menus'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
# Session 0's window is about 1028 px wide: Build and Make's 3D Print,
# Laser and CNC, which this test does not open, are hidden (the tabs' menu, mitcad#1975; the user's layout
# layer), so that the Floating title row keeps every tab it clicks, Manage
# the last, in sight beside the title row's other controls.
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[commandLayout]',
  'user="{\"version\":1,\"changes\":[],\"hiddenTabs\":[\"BUILD\",\"MAKE_PRINT\",\"MAKE_LASER\",\"MAKE_CNC\"]}"')

# The menus' entries ("..." in the source is the ellipsis character, as
# the menus show it: this file stays ASCII for Windows PowerShell).
$e = [string][char]0x2026
$construct = 'Offset Plane | Plane at Angle | Tangent Plane | Midplane | Plane Through Two Edges' +
  ' | Plane Through Three Points | Plane Tangent to Face at Point | Plane Perpendicular to Face at Point | Plane Along Path' +
  ' | Plane Through Edge and Point | Plane Normal to Path at Point | Offset Plane Through Point' +
  ' | - | Axis Through Cylinder/Cone/Torus | Axis Perpendicular at Point' +
  ' | Axis Through Two Planes | Axis Through Two Points | Axis Through Edge' +
  ' | Axis Perpendicular to Face at Point' +
  ' | - | Point at Vertex | Point Through Two Edges | Point Through Three Planes' +
  ' | Point at Center of Circle/Sphere/Torus | Point at Edge and Plane | Point Along Path | - | Coordinate System'
$rules = 'Sheet Rules | Edit Sheet Rules'
# The menus by TAB/GROUP, the tabs in their order.
$menus = [ordered]@{
  'SOLID/MODIFY' = 'Press Pull | Fillet | Chamfer | Shell | Draft' +
    ' | - | Offset Face | Move Face | Replace Face | Fill Cavity | Split Face | Delete Face' +
    ' | - | Combine | Split Body | Panelize | Scale | Convert Mesh' +
    ' | Mesh > [Mesh Section | Repair Mesh | Decimate Mesh | Remove Region | Segment Mesh | Fit Surface' +
    ' | Smooth Mesh | Combine Meshes | Offset Mesh | Align Mesh | Plane Cut]' +
    ' | - | Move/Copy | Align' +
    ' | - | Joinery > [Dogbone | Mortise and Tenon]' +
    ' | - | Material | Appearance'
  'SOLID/ASSEMBLE' = ('New Component | Replace Component...' +
    ' | - | Joint | As-Built Joint | Joint Origin | Rigid Group | Constraint | Constraint Set' +
    ' | - | Drive Joint | Motion Link | Contact Set | - | Let It Hang | - | Animate Joint | Motion Study... | Explode...').Replace('...', $e)
  'SOLID/CONSTRUCT' = $construct
  'SOLID/INSPECT' = 'Measure | Interference | Section Analysis | Print Analysis | Draft Analysis | Point Cloud Deviation | Minimum Radius | Continuity Analysis | Minimum Distance | Zebra | Curvature Map | Isocurve Analysis | Environment Map | Curvature Comb | Physical Properties | Center of Mass | Add Comment'
  'SOLID/INSERT' = ('Insert Component | Insert from Library' +
    ' | - | Insert Mesh | Insert DXF | Insert SVG | Insert Points | From an Open Design... | Component from Web... | Import...' +
    ' | Image Reference | Point Cloud | Decal').Replace('...', $e)
  'SURFACE/CREATE' = 'Create Sketch | - | Surface Extrude | Surface Revolve | Surface Sweep | Surface Loft | Surface Helix' +
    ' | - | Patch | Ruled Surface | Boundary Surface | Blend Surface | - | Offset Surface'
  'SURFACE/MODIFY' = 'Trim | Untrim | Extend | - | Stitch | Unstitch | Reverse Normal | - | Match Surface | Fair Surface' +
    ' | - | Thicken | Boundary Fill'
  'SURFACE/CONSTRUCT' = $construct + ' | - | New Construction Set | Deactivate Construction Set'
  'SHEET PARTS/CREATE' = 'Create Sketch | - | Face | Flange | Hem | Convert to Sheet Metal | Lofted Flange | - | Bend | Fold'
  'SHEET PARTS/MODIFY' = ('Sheet Cut | Perforate | Bead | - | Corner Relief | Corner Round | Corner Chamfer' +
    ' | - | Unfold | Refold | Flat Pattern | Show Flat Pattern' +
    ' | - | Export Flat Pattern... | Create Drawing of Flat Pattern... | Export Bend Table...').Replace('...', $e)
  'SHEET PARTS/RULES' = $rules
  'MANAGE/MATERIALS' = 'Edit Materials... | Edit Appearances...'.Replace('...', $e)
  'MANAGE/RULES' = $rules + ' | - | Plastic Rules'
  'MANAGE/DESIGN' = 'Recompute All | Units... | - | Design Properties...'.Replace('...', $e)
}
# The groups' own buttons, in the menus' order.
$buttons = [ordered]@{
  'SOLID/MODIFY' = 'solid.press_pull, solid.fillet, solid.chamfer, solid.shell'
  'SOLID/ASSEMBLE' = 'solid.new_component, assemble.joint, assemble.constraint_one'
  'SOLID/CONSTRUCT' = 'construct.offset_plane, construct.plane_angle, construct.midplane'
  'SOLID/INSPECT' = 'inspect.measure, inspect.section'
  'SOLID/INSERT' = 'insert.component, insert.mesh, insert.dxf'
  'SURFACE/CREATE' = 'sketch.create, surface.extrude, surface.patch'
  'SURFACE/MODIFY' = 'surface.trim, surface.thicken'
  'SURFACE/CONSTRUCT' = 'construct.offset_plane, construct.plane_angle, construct.midplane'
  'SHEET PARTS/CREATE' = 'sketch.create, sheet.face, sheet.flange, sheet.bend'
  'SHEET PARTS/MODIFY' = 'sheet.unfold, sheet.flat_pattern'
  'SHEET PARTS/RULES' = 'sheet.rules'
  'MANAGE/PARAMETERS' = 'solid.parameters, manage.favorite_parameters'
  'MANAGE/CONFIGURATIONS' = 'solid.configurations'
  'MANAGE/MATERIALS' = 'solid.materials, solid.appearances'
  'MANAGE/RULES' = 'sheet.rules, manage.plastic_rules'
  'MANAGE/DESIGN' = 'manage.recompute_all, manage.units, file.design_properties'
}

function Round([string]$Layout, [string]$Chrome) {
  Write-Host "--- The $Layout layout"
  Ui-StartApp "solid-menus-$Chrome" @("--chrome=$Chrome")
  Ui-Step 'the SOLID tab'                  { Ui-ClickTab 'SOLID' }
  $shown = 'SOLID'
  foreach ($place in $buttons.Keys) {
    Ui-ExpectLog "Ribbon group $place buttons: " "${Layout}: the $place group's buttons logged"
    $logged = "$(Ui-LastMatch ("Ribbon group $place buttons: (.*)" + '$'))"
    if ($logged -ne $buttons[$place]) { Ui-Fail "${Layout}: the $place buttons are '$logged', not '$($buttons[$place])'" }
    Write-Host "ok   ${Layout}: the $place buttons $logged"
  }
  # Session 0 may close a popup at once, after it logged its entries
  # (ui-windows-lib.ps1); Esc closes one that stays open. A group's menu
  # opens from its tab, shown first.
  foreach ($place in $menus.Keys) {
    $tab = $place.Split('/')[0]
    if ($tab -ne $shown) {
      Ui-Mark
      Ui-Step "the $tab tab"               { Ui-ClickTab "$tab" }
      Ui-ExpectNew "Ribbon tab shown: $tab" "${Layout}: the $tab tab shown"
      $shown = $tab
    }
    Ui-Mark
    Ui-Step "the $place menu"              { Ui-ClickLogged "Ribbon $place" }
    Ui-ExpectNew "Ribbon menu ${place}: " "${Layout}: the $place menu opened"
    Ui-Step 'close it (Esc)'               { Ui-Key 'Escape' }
    Ui-Sync
    $menu = "$(Ui-LastMatch ("Ribbon menu ${place}: (.*)" + '$'))"
    if ($menu -ne $menus[$place]) { Ui-Fail "${Layout}: the $place menu is '$menu', not '$($menus[$place])'" }
    Write-Host "ok   ${Layout}: the $place menu in its sections"
  }
  Ui-StopApp
}

Round 'Docked' 'docked'
Round 'Floating' 'floating'

Ui-Finish 'UI Windows solid menus test'
