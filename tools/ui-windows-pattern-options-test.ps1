# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PatternCommands.cpp app/commands/PatternOptions.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/pattern.rs tools/cli
# check-all sources: app/framework/Ribbon.cpp app/commands/CreateCommands.cpp
# check-all sources: app/framework/ToolbarPins.cpp app/framework/ToolbarPins.hpp
# Input-driven Windows UI test of the patterns' Direction Type and Distance
# Type (mitcad#545) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), on a design mitcad-cli makes: a 10 mm cube with its
# corner at (20, 20, 0) (Body1, F2.b0; its centre (25, 25, 5)).
#   0. The SOLID CREATE menu (mitcad#579) as it opens: its sections with
#      separators, the Pattern submenu with Mirror after it, the Machine
#      Elements submenu last; the ribbon's buttons as before (Create
#      Sketch, Extrude, Revolve, Hole), no split buttons for the submenus.
#      Pin to Toolbar (mitcad#1001), as tools/ui-sketch-create-menu-test.sh
#      does with the mouse on Linux: Sweep's row of the menu, the Menu key,
#      Pin to Toolbar; Sweep is pinned in the CREATE group, no separate
#      Pinned group (mitcad#1084); in session 0's 1028 px window the group
#      with Sweep's button no longer fits (mitcad#1103), so Sweep's button
#      folds and the group's own buttons (Create Sketch, Extrude, Revolve,
#      Hole) stay in place (mitcad#1111); Sweep starts from the group's
#      menu; Unpin from Toolbar from the same row's menu, the row as it was
#      (the pinned button's place and clicks are the Linux test's).
#   1. Rectangular Pattern starts with the defaults (spacing, one
#      direction). Distance Type Extent relabels the length rows Extent;
#      direction 1 along X Symmetric hides its Flip, 3 over 40 mm (20 mm
#      apart, on both sides of the cube); direction 2 along Y one direction,
#      3 over 100 mm (50 mm apart). The added line carries the options.
#   2. Circular Pattern about Z of 3 in 90 degrees, Symmetric: the copies
#      45 degrees each side of the cube.
#   3. Saved; mitcad-cli reads the stored definitions (`symmetric`,
#      `distance_type`) and the copies' centres.
#   4. Opened again: each pattern's panel shows its stored options (Extent
#      and Symmetric chosen, the Extent label; Symmetric checked).
#   5. The Pattern Type from the selection made before the command
#      (mitcad#1290), as tools/ui-pattern-options-test.sh does, on a
#      30 x 20 x 6 block with a 5 mm hole cut through it (Extrude2, F4; its
#      wall F4:side(c1) on F2.b0): the hole's wall selected in the view,
#      then Extrude2 in the timeline, start Rectangular Pattern, Circular
#      Pattern, Pattern on Path and Mirror with Pattern Type Features and
#      the cut in Objects; Body1 selected in the browser starts Rectangular
#      Pattern with Bodies.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-pattern-options-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-pattern-options'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.rectangular_pattern' = 'F7'; 'solid.circular_pattern' = 'F8';
  'solid.path_pattern' = 'F9'; 'solid.mirror' = 'F11' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'pattern-options.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [20, 20], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Sketch1 (F1), Extrude1 (F2): the block, Body1; Sketch2 (F3), Extrude2
# (F4): the hole, cut through all (part 5).
$holeScript = Join-Path $work 'hole.json'
$holeDesign = Join-Path $work 'pattern-preselect.mitcad'
Set-Content -Encoding ascii $holeScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-15, -10], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 6}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [-7, 0], "diameter": 5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}}
]
'@
$made = & $cli run $holeScript --save $holeDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $holeDesign)) { Ui-Fail "mitcad-cli (the block with a hole): $($made -join ' / ')" }
$bore = 'F4:side(c1)'

# Expect-Hidden "Panel <command> input <id>" "description": the input's
# last logged place says it is hidden (the panel logs its places again when
# its rows move).
function Expect-Hidden([string]$Text, [string]$Description) {
  for ($i = 0; $i -lt 30; $i++) {
    $line = Ui-LogLines | Where-Object { $_.Contains("$Text at ") } | Select-Object -Last 1
    if ("$line".EndsWith('(hidden)')) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the last place of '$Text' is not hidden"
}


# Show-Axes "description": the view zoomed out about the cube, a wheel step
# at a time, until the origin axes' pick places lie in the free part of the
# 1024x768 window (right of the Browser card, left of the panel; a click at
# 198,165 picked the Browser's Root row). The view fitted to the 10 mm cube
# shows the origin's axes beyond the window (the X axis' place at y 980);
# the datums log their places again whenever the camera rests, also after
# a zoom.
function Show-Axes([string]$Description) {
  # The axis input shows the origin only as its light bulb does (mitcad#1030).
  Ui-ShowOrigin
  $cube = Ui-LoggedAt 'Body F2.b0'
  for ($i = 0; $i -le 24; $i++) {
    $inside = $true
    foreach ($axis in @('x', 'y', 'z')) {
      # Ui-LoggedAt reads no minus sign: a place left of or above the
      # window is read here, as outside.
      $line = Ui-LogLines | Where-Object { $_.StartsWith("Datum $axis at ") } | Select-Object -Last 1
      $match = [regex]::Match("$line", ' at (-?\d+),(-?\d+)$')
      if (-not $match.Success) { $inside = $false; continue }
      $at = @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value)
      if ($at[0] -lt 330 -or $at[0] -gt 680 -or $at[1] -lt 180 -or $at[1] -gt 640) { $inside = $false }
    }
    if ($inside) { Write-Host "ok   $Description"; return }
    $before = @(Ui-LogLines | Where-Object { $_.StartsWith('Datum x at ') }).Count
    [UiInput]::Move($script:UiWindow, $cube[0], $cube[1], 0x04)
    Start-Sleep -Milliseconds 80
    [UiInput]::Wheel($script:UiWindow, $cube[0], $cube[1], -1)
    for ($j = 0; $j -lt 20; $j++) {
      Start-Sleep -Milliseconds 200
      if (@(Ui-LogLines | Where-Object { $_.StartsWith('Datum x at ') }).Count -gt $before) { break }
    }
    # All three axes' new places logged, as the Linux test's ui_sync.
    Ui-Sync
  }
  Ui-Fail "${Description}: the origin axes' places stayed outside the view"
}

Ui-StartApp 'pattern-options' @('--open', $design)
Ui-ExpectLog 'pattern-options.mitcad' 'the design opened'
Ui-ExpectLog 'Bodies shown: F2.b0' 'the cube shown'

Write-Host '--- 0. The SOLID CREATE menu (mitcad#579)'
# Its sections with separators, the patterns in the Pattern submenu with
# Mirror after it, the machine elements in theirs (later ones may follow
# Bearing there). Session 0 closes the popup at once (ui-windows-lib.ps1),
# after it logged its entries.
$solidCreate = 'Create Sketch | - | Extrude | Revolve | Sweep | Loft | Rib | Web | Emboss' +
  ' | - | Hole | Thread | Plastic Feature' +
  ' | - | Box | Cylinder | Sphere | Torus | Coil | Helix | Pipe | Intersection Curve | Helix Curve' +
  ' | - | Pattern > [Rectangular Pattern | Circular Pattern | Pattern on Path | Pattern at Points] | Mirror' +
  ' | - | Envelope' +
  ' | - | Machine Elements > [Gear | Gear Pair | Pulley | Belt Drive | Bearing'
Ui-Mark
Ui-Step 'the CREATE menu'                { Ui-ClickLogged 'Ribbon SOLID/CREATE' }
Ui-ExpectNew 'Ribbon menu SOLID/CREATE: ' 'the CREATE menu opened'
$menu = "$(Ui-LastMatch 'Ribbon menu SOLID/CREATE: (.*)$')"
if ($menu -ne "$solidCreate]" -and -not ($menu.StartsWith("$solidCreate | ") -and $menu.EndsWith(']'))) {
  Ui-Fail "the SOLID CREATE menu: '$menu'"
}
Write-Host 'ok   the SOLID CREATE menu in its sections and submenus'
foreach ($id in @('sketch.create', 'solid.extrude', 'solid.revolve', 'solid.hole')) {
  Ui-ExpectLog "Ribbon button $id at " "the $id button"
}
if (Ui-LogLines | Where-Object { $_.Contains('Ribbon split SOLID/CREATE/') }) {
  Ui-Fail 'a split button for a SOLID CREATE submenu'
}
Write-Host 'ok   no split buttons for the Pattern and Machine Elements submenus'
# The menu stays open in session 0 and takes the keys: closed before F7
# (with it open, F7 started nothing).
Ui-Step 'close the menu (Esc)'           { Ui-Key 'Escape' }
Ui-Sync

Write-Host '--- 0b. Pin to Toolbar (mitcad#1001)'
# Expect-CreateRow "description": CREATE's own buttons where they were
# logged before Sweep was pinned ($createRow), none of them gone from a
# layout logged since the mark, and no pinned button in sight
# (mitcad#1111).
$createOwn = @('sketch.create', 'solid.extrude', 'solid.revolve', 'solid.hole')
$createRow = @{}
foreach ($id in $createOwn) { $createRow[$id] = "$(Ui-LoggedAt "Ribbon button $id")" }
function Expect-CreateRow([string]$Description) {
  $new = @(Ui-NewLines)
  if ($new | Where-Object { $_.Contains('Ribbon pinned SOLID/') }) {
    Ui-Fail "${Description}: a pinned button in sight: '$($new | Where-Object { $_.Contains('Ribbon pinned SOLID/') } | Select-Object -Last 1)'"
  }
  $relaid = [bool]($new | Where-Object { $_.Contains('Ribbon SOLID/CREATE at ') })
  foreach ($id in $createOwn) {
    if ($relaid -and -not ($new | Where-Object { $_.Contains("Ribbon button $id at ") })) {
      Ui-Fail "${Description}: the places logged again without CREATE's $id"
    }
    $at = "$(Ui-LoggedAt "Ribbon button $id")"
    if ($at -ne $createRow[$id]) { Ui-Fail "${Description}: $id at $at, before at $($createRow[$id])" }
  }
  Write-Host "ok   $Description"
}
# The menu takes the keys in session 0: Sweep is its fourth row (the
# separator is skipped); the Menu key opens the row's menu with Pin to
# Toolbar chosen.
Ui-Mark
Ui-Step 'the CREATE menu'                { Ui-ClickLogged 'Ribbon SOLID/CREATE' }
Ui-ExpectNew 'Ribbon menu SOLID/CREATE: ' 'the CREATE menu opened'
Ui-Step 'Sweep''s row, the Menu key'      { Ui-Key 'Down', 'Down', 'Down', 'Down', 'Menu' }
Ui-ExpectNew 'Toolbar menu SOLID/CREATE solid.sweep: Pin to Toolbar | ' 'Sweep''s row menu by the Menu key'
Ui-Step 'Pin to Toolbar (Enter)'         { Ui-Key 'Return' }
Ui-ExpectNew 'Pinned to toolbar: Sweep (Solid)' 'Sweep pinned'
Ui-ExpectNew 'Ribbon group SOLID/CREATE pinned: solid.sweep' 'Sweep in the CREATE group (mitcad#1084)'
Ui-Sync
# Session 0's screen is 1024 px wide and the window cannot grow past it
# (the Linux test widens its window to 1600 px for the pinned button): with
# Sweep's button the CREATE group no longer fits beside the other groups'
# names (mitcad#1103), so Sweep's button folds, not the group: Create
# Sketch, Extrude, Revolve and Hole stay where they were (mitcad#1111).
# The pinned button's place, its click and its right-click are the Linux
# test's.
Ui-ExpectNew 'Ribbon pinned folded SOLID/CREATE: solid.sweep' 'Sweep''s button folded in the 1028 px window (mitcad#1111)'
Start-Sleep -Milliseconds 300
Ui-Sync
Expect-CreateRow 'CREATE''s own buttons in place, Sweep''s button folded'
if (Ui-LogLines | Where-Object { $_.Contains('/PINNED') }) { Ui-Fail 'a separate Pinned group' }
Write-Host 'ok   no Pinned group'
# The folded Sweep from the group's menu, which lists it as before: its
# row by the keys, Enter starts it.
Ui-Step 'close the menu (Esc)'           { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'the CREATE menu'                { Ui-ClickLogged 'Ribbon SOLID/CREATE' }
Ui-ExpectNew 'Ribbon menu SOLID/CREATE: ' 'the CREATE menu opened with Sweep folded'
if (-not "$(Ui-LastMatch 'Ribbon menu SOLID/CREATE: (.*)$')".Contains(' | Revolve | Sweep | Loft | ')) {
  Ui-Fail 'the folded Sweep not in the CREATE menu'
}
Write-Host 'ok   the folded Sweep in the CREATE menu'
Ui-Step 'Sweep''s row (Enter)'            { Ui-Key 'Down', 'Down', 'Down', 'Down', 'Return' }
Ui-ExpectNew 'Command Sweep started' 'the folded Sweep starts from the CREATE menu'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Sweep cancelled' 'Sweep cancelled'
# Unpinned from the same row of the group's menu: its menu by the Menu key
# opens with Unpin from Toolbar chosen; the row as it was before the pin.
Ui-Mark
Ui-Step 'the CREATE menu'                { Ui-ClickLogged 'Ribbon SOLID/CREATE' }
Ui-ExpectNew 'Ribbon menu SOLID/CREATE: ' 'the CREATE menu opened'
Ui-Step 'Sweep''s row, the Menu key'      { Ui-Key 'Down', 'Down', 'Down', 'Down', 'Menu' }
Ui-ExpectNew 'Toolbar menu SOLID/CREATE solid.sweep: Unpin from Toolbar | ' 'Sweep''s row menu: Unpin from Toolbar'
Ui-Step 'Unpin from Toolbar (Enter)'     { Ui-Key 'Return' }
Ui-ExpectNew 'Unpinned: Sweep (Solid)' 'Sweep unpinned'
Ui-ExpectNew 'Ribbon group SOLID/CREATE pinned: none' 'the CREATE group without pins'
Ui-ExpectNew 'Ribbon pinned folded SOLID/CREATE: none' 'no folded pins left'
Start-Sleep -Milliseconds 300
Ui-Sync
Expect-CreateRow 'unpinned: CREATE''s row as before the pin, no pinned button left'
Ui-Step 'close the menu (Esc)'           { Ui-Key 'Escape' }
Ui-Sync

Write-Host '--- 1. A rectangular pattern: symmetric along X, by extent'
Ui-Mark
Ui-Step 'rectangular pattern (F7)'       { Ui-Key 'F7' }
Ui-ExpectNew 'Command Rectangular Pattern started' 'the panel open'
Ui-ExpectNew 'Panel Rectangular Pattern choices: object_type=bodies, compute=adjust, distance_type=spacing, direction_type1=one, direction_type2=one' 'the defaults: spacing, one direction'
Ui-Step 'pick Body1'                     { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew 'Rectangular Pattern Objects: 1 body [body F2.b0]' 'the cube picked'
Ui-Step 'the direction input'            { Ui-ClickLogged 'Panel Rectangular Pattern input direction1' }
Show-Axes 'the origin axes in the view'
Ui-Step 'pick the X axis'                { Ui-ClickLogged 'Datum x' }
Ui-ExpectNew 'Rectangular Pattern Direction 1: 1 axis [axis x]' 'X picked'
Ui-ExpectNew 'Preview Rectangular Pattern: ok' 'previewed' 30
Ui-Mark
Ui-Step 'Distance Type: Extent'          { Ui-WheelLogged 'Panel Rectangular Pattern input distance_type' -1 }
Ui-ExpectNew 'Rectangular Pattern: Distance Type = Extent' 'extent chosen'
Ui-ExpectNew 'Panel Rectangular Pattern label distance1: Extent' 'the length row says Extent'
Ui-ExpectNew 'Panel Rectangular Pattern label distance2: Extent 2' 'the second one too'
Ui-ExpectNew 'Preview Rectangular Pattern: ok' 'the preview follows' 30
Ui-Mark
Ui-Step 'Direction Type: Symmetric'      { Ui-WheelLogged 'Panel Rectangular Pattern input direction_type1' -1 }
Ui-ExpectNew 'Rectangular Pattern: Direction Type = Symmetric' 'symmetric chosen'
Ui-ExpectNew 'Preview Rectangular Pattern: ok' 'the preview follows' 30
Expect-Hidden 'Panel Rectangular Pattern input flip1' 'Flip hidden while symmetric'
Ui-Mark
Ui-Step 'extent 40'                      { Ui-TypeIn 'Panel Rectangular Pattern input distance1' '40' }
Ui-ExpectNew 'Rectangular Pattern Extent: 40 = 40 mm' '40 mm from the first to the last'
Ui-ExpectNew 'Preview Rectangular Pattern: ok' 'previewed' 30
Ui-Step 'the second direction input'     { Ui-ClickLogged 'Panel Rectangular Pattern input direction2' }
Ui-Step 'pick the Y axis'                { Ui-ClickLogged 'Datum y' }
Ui-ExpectNew 'Rectangular Pattern Direction 2: 1 axis [axis y]' 'Y picked'
Ui-Step 'quantity 3'                     { Ui-TypeIn 'Panel Rectangular Pattern input quantity2' '3' }
Ui-ExpectNew 'Rectangular Pattern Quantity 2: 3' 'three along Y'
Ui-Mark
Ui-Step 'extent 100'                     { Ui-TypeIn 'Panel Rectangular Pattern input distance2' '100' }
Ui-ExpectNew 'Rectangular Pattern Extent 2: 100 = 100 mm' '100 mm from the first to the last'
Ui-ExpectNew 'Preview Rectangular Pattern: ok' 'previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added rectangular pattern of 1 bodies: direction 1 symmetric, quantity 3, extent 40 mm; direction 2 one direction, quantity 3, extent 100 mm' 'the definition logged' 30
Ui-ExpectNew 'Added Rectangular Pattern as F3' 'RectangularPattern1 added' 30
Ui-ExpectVolume 'New body \S+ \(F3\.b7\): volume ([0-9.]+) mm3' 1000 'the eighth copy'

Write-Host '--- 2. A circular pattern: symmetric about Z in 90 degrees'
Ui-Mark
Ui-Step 'circular pattern (F8)'          { Ui-Key 'F8' }
Ui-ExpectNew 'Command Circular Pattern started' 'the panel open'
Ui-ExpectNew 'Panel Circular Pattern checks: symmetric=off' 'not symmetric by default'
Ui-Step 'pick Body1'                     { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew 'Circular Pattern Objects: 1 body [body F2.b0]' 'the cube picked'
Ui-Step 'the axis input'                 { Ui-ClickLogged 'Panel Circular Pattern input axis' }
Show-Axes 'the origin axes in the view'
Ui-Step 'pick the Z axis'                { Ui-ClickLogged 'Datum z' }
Ui-ExpectNew 'Circular Pattern Axis: 1 axis [axis z]' 'Z picked'
Ui-Step 'quantity 3'                     { Ui-TypeIn 'Panel Circular Pattern input quantity' '3' }
Ui-Step 'angle 90'                       { Ui-TypeIn 'Panel Circular Pattern input angle' '90' }
Ui-ExpectNew 'Circular Pattern Total Angle: 90 = 90 deg' 'a quarter turn'
Ui-Mark
Ui-Step 'Symmetric'                      { Ui-ClickLogged 'Panel Circular Pattern input symmetric' }
Ui-ExpectNew 'Circular Pattern: Symmetric = on' 'symmetric checked'
Ui-ExpectNew 'Preview Circular Pattern: ok' 'the preview follows' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added circular pattern of 1 bodies: symmetric, quantity 3, angle 90 deg' 'the definition logged' 30
Ui-ExpectNew 'Added Circular Pattern as F4' 'CircularPattern1 added' 30

Write-Host '--- 3. The stored definitions and the copies'' places'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved' 30
# Element e of the rectangular pattern is body F3.b(e-1), e = k1 + 3 k2:
# along X at 0, +20, -20 (symmetric), along Y at 0, +50, +100. The circular
# pattern's elements at +45 and -45 degrees.
$check = Join-Path $work 'check.json'
Set-Content -Encoding ascii $check @'
[
  {"expect": {"bodies": 11}},
  {"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {
    "type": "rectangular_pattern", "def": {"direction1": {"symmetric": true}, "distance_type": "extent"}}}},
  {"expect": {"query": {"query": "feature", "uid": "F4"}, "result": {
    "type": "circular_pattern", "def": {"symmetric": true}}}},
  {"expect": {"body": "F3.b0", "center": [45, 25, 5]}},
  {"expect": {"body": "F3.b1", "center": [5, 25, 5]}},
  {"expect": {"body": "F3.b2", "center": [25, 75, 5]}},
  {"expect": {"body": "F3.b7", "center": [5, 125, 5]}},
  {"expect": {"body": "F4.b0", "center": [0, 35.35533905932738, 5]}},
  {"expect": {"body": "F4.b1", "center": [35.35533905932738, 0, 5]}}
]
'@
$checked = & $cli run $check --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved design: $($checked -join ' / ')" }
Write-Host 'ok   stored: direction 1 symmetric, extent; the circular pattern symmetric; the copies on both sides'

Write-Host '--- 4. Opened again: the panels show the stored options'
# A plain double-click on the cell: its first click selects the pattern's
# bodies, and the window keeps its size (mitcad#560: it grew, and the
# second press landed beside the cell).
Ui-StartApp 'pattern-options-reopened' @('--open', $design)
Ui-ExpectLog 'pattern-options.mitcad' 'the design opened again'
Ui-Mark
Ui-Step 'edit RectangularPattern1'       { Ui-DoubleClickLogged 'Timeline RectangularPattern1' }
Ui-ExpectNew 'Editing F3 with Rectangular Pattern' 'editing the pattern'
Ui-ExpectNew 'Panel Rectangular Pattern choices: object_type=bodies, compute=adjust, distance_type=extent, direction_type1=symmetric, direction_type2=one' 'Extent and Symmetric shown'
Ui-ExpectNew 'Panel Rectangular Pattern label distance1: Extent' 'the length row says Extent'
Ui-ExpectNew 'Preview Rectangular Pattern: ok' 'previewed' 30
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Rectangular Pattern cancelled' 'left as it was'
Ui-Mark
Ui-Step 'edit CircularPattern1'          { Ui-DoubleClickLogged 'Timeline CircularPattern1' }
Ui-ExpectNew 'Editing F4 with Circular Pattern' 'editing the circular pattern'
Ui-ExpectNew 'Panel Circular Pattern checks: symmetric=on' 'Symmetric checked'
Ui-ExpectNew 'Preview Circular Pattern: ok' 'previewed' 30
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Circular Pattern cancelled' 'left as it was'

Write-Host '--- 5. The Pattern Type from the selection made before (mitcad#1290)'
# Starts-With command key type objects what: the command started by its
# shortcut with the selection made before: its panel's Pattern Type is
# type and its Objects row holds objects. Cancelled, and the selection
# cleared for the next case.
function Starts-With([string]$Command, [string]$Key, [string]$Type, [string]$Objects, [string]$What) {
  Ui-Mark
  Ui-Step "$Command ($Key)"              { Ui-Key $Key }
  Ui-ExpectNew "Command $Command started" "$Command started"
  Ui-ExpectNew "Panel $Command choices: object_type=$Type," "${What}: $Command's Pattern Type $Type"
  Ui-ExpectNew "$Command Objects: $Objects" "${What}: $Objects in $Command's Objects"
  Ui-Mark
  Ui-Step 'cancel (Esc)'                 { Ui-Key 'Escape' }
  Ui-ExpectNew "Command $Command cancelled" "$Command cancelled"
  Ui-Step 'clear the selection (Esc)'    { Ui-Key 'Escape' }
  Ui-Sync
}
# Select-Wall: the hole's wall selected in the view. Measure first for the
# log's pick place of the wall: it takes faces, edges and vertices, as an
# idle click does, so its place for the wall is one no edge or vertex
# takes (the patterns' Features input logs places among faces only, where
# the rim may take an idle click; Fillet's, among faces and edges, lay on
# the rim's seam vertex on Windows). Cancelled before the click.
function Select-Wall {
  Ui-Mark
  Ui-Step 'measure (I)'                  { Ui-Key 'i' }
  Ui-ExpectNew "Pick face F2.b0/$bore at" 'the hole''s wall can be picked'
  $wall = Ui-PickAt "face F2.b0/$bore"
  Ui-Mark
  Ui-Step 'cancel (Esc)'                 { Ui-Key 'Escape' }
  Ui-ExpectNew 'Command Measure cancelled' 'Measure cancelled'
  Ui-Mark
  Ui-Step 'select the hole''s wall'      { Ui-Click $wall[0] $wall[1] }
  Ui-ExpectNew "Selected: 1 face [face $bore of F2.b0" 'the hole''s wall selected'
}
$patterns = @(@('Rectangular Pattern', 'F7'), @('Circular Pattern', 'F8'), @('Pattern on Path', 'F9'), @('Mirror', 'F11'))
Ui-StartApp 'pattern-preselect' @('--open', $holeDesign)
Ui-ExpectLog 'pattern-preselect.mitcad' 'the block with a hole opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the block shown'
foreach ($pattern in $patterns) {
  Select-Wall
  Starts-With $pattern[0] $pattern[1] 'features' '1 feature [feature F4]' 'the hole''s wall'
}
foreach ($pattern in $patterns) {
  Ui-Mark
  Ui-Step 'select Extrude2 in the timeline' { Ui-ClickLogged 'Timeline Extrude2' }
  Ui-ExpectNew 'Selected: 1 feature [feature F4' 'the hole''s cut selected'
  Starts-With $pattern[0] $pattern[1] 'features' '1 feature [feature F4]' 'the cut in the timeline'
}
Ui-Mark
Ui-Step 'select Body1 in the browser'    { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Selected: 1 body [body F2.b0]' 'Body1 selected'
Starts-With 'Rectangular Pattern' 'F7' 'bodies' '1 body [body F2.b0]' 'a body'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows pattern options test'
