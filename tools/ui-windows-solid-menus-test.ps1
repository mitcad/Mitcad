# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Command.hpp app/MainWindow.cpp
# check-all sources: app/commands/ModifyCommands.cpp app/commands/SheetCommands.cpp app/MainWindowSheet.cpp
# check-all sources: app/commands/SurfaceCommands.cpp app/commands/ConstructCommands.cpp
# check-all sources: app/commands/JointCommands.cpp app/commands/PrimitiveCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/files/MainWindowLibraries.cpp
# The SOLID tab's menus in their sections (mitcad#1778) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), in the Docked
# and the Floating layout, as tools/ui-solid-menus-test.sh does on Linux:
# MODIFY, SHEET METAL, SURFACE, ASSEMBLE and CONSTRUCT as they open, every
# entry in its place and a separator ("-") between two sections
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

# The menus' entries ("..." in the source is the ellipsis character, as
# the menus show it: this file stays ASCII for Windows PowerShell).
$e = [string][char]0x2026
$menus = [ordered]@{
  'MODIFY' = ('Press Pull | Fillet | Chamfer | Shell | Draft' +
    ' | - | Offset Face | Move Face | Replace Face | Split Face | Delete Face' +
    ' | - | Combine | Split Body | Scale | Convert Mesh' +
    ' | - | Move/Copy | Align' +
    ' | - | Dogbone | Mortise and Tenon' +
    ' | - | Material | Appearance | Edit Appearances...' +
    ' | - | Edit Parameters... | Configurations...').Replace('...', $e)
  'SHEET METAL' = 'Sheet Rules | Edit Sheet Rules' +
    ' | - | Face | Flange' +
    ' | - | Bend | Fold | Corner Relief' +
    ' | - | Corner Round | Corner Chamfer | Sheet Cut' +
    ' | - | Unfold | Refold | Flat Pattern | Show Flat Pattern'
  'SURFACE' = 'Surface Extrude | Surface Revolve | Surface Sweep | Surface Loft | Surface Helix' +
    ' | - | Patch | Ruled Surface | Boundary Surface | Blend Surface' +
    ' | - | Extend | Trim | Untrim | Offset Surface' +
    ' | - | Match Surface | Fair Surface | Reverse Normal' +
    ' | - | Stitch | Unstitch | Thicken'
  'ASSEMBLE' = ('New Component' +
    ' | - | Joint | As-Built Joint | Joint Origin | Rigid Group | Assembly Constraint' +
    ' | - | Drive Joint | Motion Link | Contact Set | Animate Joint | Motion Study...' +
    ' | - | Insert Component | Insert from Library').Replace('...', $e)
  'CONSTRUCT' = 'Offset Plane | Plane at Angle | Tangent Plane | Midplane | Plane Through Two Edges' +
    ' | Plane Through Three Points | Plane Tangent to Face at Point | Plane Along Path' +
    ' | Plane Through Edge and Point | Plane Normal to Path at Point | Offset Plane Through Point' +
    ' | - | Axis Through Cylinder/Cone/Torus | Axis Perpendicular at Point' +
    ' | Axis Through Two Planes | Axis Through Two Points | Axis Through Edge' +
    ' | Axis Perpendicular to Face at Point' +
    ' | - | Point at Vertex | Point Through Two Edges | Point Through Three Planes' +
    ' | Point at Center of Circle/Sphere/Torus | Point at Edge and Plane | Point Along Path | - | Coordinate System'
}
# The groups' own buttons, in the menus' order.
$buttons = [ordered]@{
  'MODIFY' = 'solid.press_pull, solid.fillet, solid.chamfer, solid.shell, solid.combine'
  'SHEET METAL' = 'sheet.face, sheet.flange, sheet.bend, sheet.unfold, sheet.flat_pattern'
  'SURFACE' = 'surface.extrude, surface.patch, surface.trim, surface.thicken'
  'ASSEMBLE' = 'solid.new_component, assemble.joint, assemble.constraint, insert.component'
  'CONSTRUCT' = 'construct.offset_plane, construct.plane_angle, construct.midplane'
}

function Round([string]$Layout, [string]$Chrome) {
  Write-Host "--- The $Layout layout"
  Ui-StartApp "solid-menus-$Chrome" @("--chrome=$Chrome")
  Ui-Step 'the SOLID tab'                  { Ui-ClickLogged 'Ribbon tab SOLID' }
  foreach ($group in $buttons.Keys) {
    Ui-ExpectLog "Ribbon group SOLID/$group buttons: " "${Layout}: the $group group's buttons logged"
    $logged = "$(Ui-LastMatch ("Ribbon group SOLID/$group buttons: (.*)" + '$'))"
    if ($logged -ne $buttons[$group]) { Ui-Fail "${Layout}: the $group buttons are '$logged', not '$($buttons[$group])'" }
    Write-Host "ok   ${Layout}: the $group buttons $logged"
  }
  # Session 0 may close a popup at once, after it logged its entries
  # (ui-windows-lib.ps1); Esc closes one that stays open.
  foreach ($group in $menus.Keys) {
    Ui-Mark
    Ui-Step "the $group menu"              { Ui-ClickLogged "Ribbon SOLID/$group" }
    Ui-ExpectNew "Ribbon menu SOLID/${group}: " "${Layout}: the $group menu opened"
    Ui-Step 'close it (Esc)'               { Ui-Key 'Escape' }
    Ui-Sync
    $menu = "$(Ui-LastMatch ("Ribbon menu SOLID/${group}: (.*)" + '$'))"
    if ($menu -ne $menus[$group]) { Ui-Fail "${Layout}: the $group menu is '$menu', not '$($menus[$group])'" }
    Write-Host "ok   ${Layout}: the $group menu in its sections"
  }
  Ui-StopApp
}

Round 'Docked' 'docked'
Round 'Floating' 'floating'

Ui-Finish 'UI Windows solid menus test'
