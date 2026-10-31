# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser app/files app/MainWindow.cpp core/model/src/drawing core/model-drawing/src
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing.rs core/model/src/document/drawings.rs tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_tables.rs
# check-all sources: core/model-api/src/drawing_templates.rs
# check-all sources: app/framework/Ribbon.cpp app/framework/FloatingLayout.cpp app/framework/GlassCard.cpp
# Input-driven Windows UI test of drawings kept in the design (mitcad#707)
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# as tools/ui-drawings-test.sh does on Linux, on a 40 x 30 x 20 block
# mitcad-cli makes (Body1, F2.b0):
#   1. Create Drawing (a shortcut of the test's): its dialog (the ISO
#      template), Drawing1 in the root's Drawings folder, its sheet over
#      the 3D view: the whole of it, with the ribbon's DRAWING tab alone
#      (Solid and Build hidden, mitcad#1008).
#   2. Base View at a click (the front at 2:1), a projected view right of
#      it, a section across it and a detail of its corner, each placed by
#      a click; a break; the detail deleted (Delete) and back (Ctrl+Z); a
#      second sheet (+); a dimension of the block's height picked on its
#      edges and placed by a click (phase 2).
#   3. Close Drawing (Solid and Build back, Solid shown); Extrude1 made 35
#      high in its panel (the dimension follows: 35); the drawing opened
#      again from the browser on the Build tab: the front view has grown
#      upward; Close Drawing goes back to Build (mitcad#1008).
#   4. Saved and opened again: the drawing with its four views.
#   5. Export... PDF: a PDF of the two sheets; Export... DWG: an R2000
#      DWG file per sheet (mitcad#728); Add Revision: the revision
#      table and the title block show it (phase 4).
#   6. An assembly mitcad-cli makes (Component1 placed twice, Component2
#      once): a front view, a parts list of it placed by a click and a
#      balloon on Component1's second placement (phase 3).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawings-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawings'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'file.create_drawing' = 'F9' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'block.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Sheet-At x y: the window point of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)).
function Sheet-At([double]$X, [double]$Y) {
  $view = Ui-LastMatch 'Drawing sheet view ([-0-9.,]+ [-0-9.,]+ [-0-9.,]+)'
  if (-not $view) { Ui-Fail 'the app did not log the drawing''s sheet' }
  $p = $view -split ' ' | ForEach-Object { , ($_ -split ',' | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) }) }
  $px = $p[0][0] + ($p[1][0] - $p[0][0]) * $X / 100 + ($p[2][0] - $p[0][0]) * $Y / 100
  $py = $p[0][1] + ($p[1][1] - $p[0][1]) * $X / 100 + ($p[2][1] - $p[0][1]) * $Y / 100
  return @([int][Math]::Round($px), [int][Math]::Round($py))
}

function Sheet-Click([double]$X, [double]$Y) {
  $at = Sheet-At $X $Y
  Ui-Click $at[0] $at[1]
  # The click handled before the next one, as the Linux test's sheet_click
  # syncs (mitcad#870: a fixed 300 ms before).
  Ui-Sync
}

# View-Y id: the window y of the middle of a view's box, as logged last.
function View-Y([int]$Id) {
  $at = Ui-LoggedAt "Drawing view $Id"
  return $at[1]
}

# Settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait, as the Linux test's settled (a
# panel or a tab bar just built is laid out again once; a click must not
# use the place it had before).
function Settled([string]$Text) {
  for ($i = 0; $i -lt 20; $i++) {
    $before = Ui-LastLineWith "$Text at "
    Ui-Sync
    Start-Sleep -Milliseconds 300
    $after = Ui-LastLineWith "$Text at "
    if ($after -and $before -eq $after) { return }
  }
  Ui-Fail "the place of '$Text' did not settle"
}

# Drawing-Tool "name" "group": the drawing tool started from the ribbon's
# DRAWING tab (mitcad#1008), by its button when the tab shows it, else by
# its row in its group's menu. In session 0's 1024x768 desktop the ribbon
# collapses its right groups (Dimensions, Tables, ...) to their names
# (Ribbon::fitGroups): the drawing then logs no place for their tools, only
# the group's ("Drawing group Dimensions at x,y"), whose menu has them
# ("Ribbon menu DRAWING/DIMENSIONS: Dimension | Ordinate | Baseline"). The
# drawing logs all its places together, from "Drawing area", whenever one
# changes: the last such lines say what the tab shows now.
function Drawing-Tool([string]$Name, [string]$Group) {
  Settled "Drawing group $Group"
  $lines = @(Ui-LogLines)
  $start = -1
  for ($i = $lines.Count - 1; $i -ge 0; $i--) {
    if ($lines[$i].Contains('Drawing area ')) { $start = $i; break }
  }
  $shown = $start -ge 0 -and @($lines | Select-Object -Skip $start | Where-Object { $_.Contains("Drawing tool $Name at ") }).Count -gt 0
  if ($shown) {
    # Not $Name in the action: Ui-Step's own parameter of that name hides it.
    $button = "Drawing tool $Name"
    Settled $button
    Ui-Step $Name                           { Ui-ClickLogged $button }
    return
  }
  $path = 'DRAWING/' + $Group.ToUpperInvariant()
  Ui-Step "the $Group menu (the group shows its name only)" { Ui-ClickLogged "Drawing group $Group" }
  Ui-ExpectNew "Ribbon menu ${path}: " "the $Group group's menu"
  $menu = "$(Ui-LastLineWith "Ribbon menu ${path}: ")"
  $entries = $menu.Substring($menu.IndexOf("Ribbon menu ${path}: ") + "Ribbon menu ${path}: ".Length)
  # Its rows as the keys go through them (no separators); the drawing's Parts
  # List is "Parts List (Drawing)" there (mitcad#718).
  $rows = @($entries -split ' \| ' | Where-Object { $_ -ne '-' })
  $row = -1
  for ($i = 0; $i -lt $rows.Count; $i++) {
    if ($rows[$i] -eq $Name -or $rows[$i].StartsWith("$Name (")) { $row = $i; break }
  }
  if ($row -lt 0) { Ui-Fail "no $Name in the $Group group's menu: $entries" }
  Ui-Step "$Name (Down x$($row + 1), Enter)" { Ui-Key (@('Down') * ($row + 1)); Ui-Key 'Return' }
}

# Area-IsView: the drawing's area, as logged last ("Drawing area x,y WxH,
# view x,y WxH"), is the 3D view's (mitcad#1008: edge to edge), as the
# Linux test's area_is_view.
function Area-IsView {
  $line = $null
  for ($i = 0; $i -lt 20; $i++) {
    $line = Ui-LastLineWith 'Drawing area '
    if ($line -and $line -match 'Drawing area (\S+ \S+), view (\S+ \S+)' -and $Matches[1] -eq $Matches[2]) {
      Write-Host "ok   the drawing takes the whole view: $($Matches[1])"
      return
    }
    Ui-Sync
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "the drawing's area is not the view's: $line"
}

Ui-StartApp 'drawings' @('--open', $design)
Ui-ExpectLog 'block.mitcad' 'the design opened'

Write-Host '--- 1. Create Drawing'
Ui-Mark
Ui-Step 'Create Drawing (F9)'             { Ui-Key 'F9' }
Ui-ExpectNew 'Create Drawing dialog opened' 'its dialog asks for the sheet'
Ui-Step 'OK: A3, landscape, first angle'  { Ui-FocusDialog '^Create Drawing$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Create Drawing dialog: template iso, A3' 'the ISO template on its A3'
Ui-ExpectNew 'Drawing created: D1 Drawing1 in C0' "Drawing1 in the root's folder"
Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'its sheet over the view'
Ui-ExpectNew 'Drawing tool Base View at' 'the tools laid out'
Ui-ExpectNew 'Drawing sheet view ' 'the sheet placed'
# The drawing takes the whole workspace (mitcad#1008): the ribbon's DRAWING
# tab alone, Close Drawing on it, the drawing's area the 3D view's.
Ui-ExpectNew 'Ribbon tabs: DRAWING (shown DRAWING)' 'the DRAWING tab alone: Solid and Build hidden'
Ui-ExpectNew 'Ribbon button drawing.close at' 'Close Drawing on the ribbon'
Area-IsView

Write-Host '--- 2. Views'
Ui-ExpectNew 'Drawing tool: Base View' 'Base View on for the new drawing (mitcad#725)'
Ui-Mark
Ui-Step 'click the sheet at 120, 120'     { Sheet-Click 120 120 }
Ui-ExpectNew '"type":"base"' 'the base view asked for'
Ui-ExpectNew 'Drawing view 1 at' 'the front view drawn'
Ui-ExpectNew 'Drawing tool: Projected View' 'projected views follow'
Ui-Mark
Ui-Step 'click right of it'               { Sheet-Click 215 120 }
Ui-ExpectNew '"type":"projected"' 'a projected view asked for'
Ui-ExpectNew 'Drawing view 2 at' 'the right view drawn'
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Ui-ExpectNew 'Drawing tool: selection' 'back to the selection'
Ui-Mark
Ui-Step 'Section View'                    { Ui-ClickLogged 'Drawing tool Section View' }
Ui-ExpectNew 'Drawing tool: Section View' 'the section tool is on'
Ui-Step 'the line on the view'            { Sheet-Click 120 110; Sheet-Click 120 150 }
Ui-ExpectNew 'Drawing placing view' 'the line drawn: its view is placed next'
Ui-Step 'where the section goes'          { Sheet-Click 48 120 }
Ui-ExpectNew '"type":"section"' 'the section asked for'
Ui-ExpectNew 'Drawing view 3 at' 'the section drawn'
Ui-Mark
Ui-Step 'Detail View'                     { Ui-ClickLogged 'Drawing tool Detail View' }
Ui-ExpectNew 'Drawing tool: Detail View' 'the detail tool is on'
Ui-Step 'the circle on the view'          { Sheet-Click 86 106; Sheet-Click 96 106 }
Ui-ExpectNew 'Drawing placing view' 'the circle drawn: its view is placed next'
Ui-Step 'where the detail goes'           { Sheet-Click 230 70 }
Ui-ExpectNew '"type":"detail"' 'the detail asked for'
Ui-ExpectNew 'Drawing view 4 at' 'the detail drawn'
Ui-Mark
Ui-Step 'Break'                           { Ui-ClickLogged 'Drawing tool Break' }
Ui-ExpectNew 'Drawing tool: Break' 'the break tool is on'
Ui-Step 'the band on the view'            { Sheet-Click 140 120; Sheet-Click 150 120 }
Ui-ExpectNew '"add_break":' 'a break asked for'
Ui-ExpectNew 'Drawing sheet drawn: 4 views' 'the front view drawn shorter'
Ui-Mark
Ui-Step 'select the detail'               { Ui-ClickLogged 'Drawing view 4' }
Ui-ExpectNew 'Drawing view 4 selected' 'the detail selected'
Ui-Step 'Delete'                          { Ui-Key 'Delete' }
Ui-ExpectNew '"cmd":"delete_drawing_view"' 'its deletion asked for'
Ui-ExpectNew 'Drawing sheet drawn: 3 views' 'the detail gone'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                   { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Drawing sheet drawn: 4 views' 'the detail back, the drawing still open'
Ui-Mark
Ui-Step '+ (a sheet)'                     { Ui-ClickLogged 'Drawing Add Sheet' }
Ui-ExpectNew '"cmd":"add_drawing_sheet"' 'a sheet asked for'
Ui-ExpectNew 'Drawing Sheet tab 2 at' 'its tab'
Ui-Mark
Settled 'Drawing Sheet tab 1'
Ui-Step 'the first sheet again'           { Ui-ClickLogged 'Drawing Sheet tab 1' }
Ui-ExpectNew 'Drawing sheet tab pressed: 1' 'the press on its tab, as Qt finds it (mitcad#725)'
Ui-ExpectNew 'Drawing sheet 1 shown' 'the first sheet shown'
Ui-ExpectNew 'Drawing sheet drawn: 4 views' 'its four views'
if (Ui-LogLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }
# A dimension of the block's height, picked on the front view's bottom and
# top edges (y 100 and 140 on the sheet at 2:1) and placed by a click.
Ui-Mark
Drawing-Tool 'Dimension' 'Dimensions'
Ui-ExpectNew 'Drawing tool: Dimension' 'the dimension tool is on'
Ui-Step 'the bottom edge'                 { Sheet-Click 95 100.5 }
Ui-ExpectNew 'Drawing picked line (1)' 'the bottom edge picked'
Ui-Step 'the top edge'                    { Sheet-Click 95 139.5 }
Ui-ExpectNew 'Drawing placing view' 'its place is clicked next'
Ui-Step 'where it goes'                   { Sheet-Click 172 120 }
Ui-ExpectNew '"type":"linear"' 'a linear dimension asked for'
Ui-ExpectNew 'Drawing annotations drawn: 1 = 20' 'drawn: the block is 20 high'
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
if (Ui-LogLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }

Write-Host '--- 3. The model changes, the drawing follows'
$before = View-Y 1
Ui-Mark
Ui-Step 'Close Drawing'                   { Ui-ClickLogged 'Drawing Close Drawing' }
Ui-ExpectNew 'Drawing closed' 'the 3D view again'
Ui-ExpectNew 'Ribbon tabs: SOLID, BUILD (shown SOLID)' "the model's tabs back, the one it was opened from shown"
Ui-Mark
Ui-Step 'edit Extrude1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Extrude1' }
Ui-ExpectNew ' with Extrude' 'editing Extrude1'
Ui-Step 'distance 35'                     { Ui-TypeIn 'Panel Extrude input distance' '35' }
Ui-WaitIdle
Ui-Step 'OK (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'Extrude1 edited'
# Opened from the Build tab, the drawing closes back to it (mitcad#1008).
Ui-Mark
Ui-Step 'the Build tab'                   { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-ExpectNew 'Ribbon BUILD/PROFILE at' 'the Build tab shown'
Ui-Mark
Ui-Step 'open Drawing1 (double-click)'    { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 2' 'the drawing opened again'
Ui-ExpectNew 'Ribbon tabs: DRAWING (shown DRAWING)' 'the DRAWING tab alone'
Ui-ExpectNew 'Drawing sheet drawn: 4 views' 'its four views'
Ui-ExpectNew 'Drawing view 1 at' 'the front view placed'
$after = View-Y 1
if ($after -ge $before) { Ui-Fail "the front view did not grow upward ($before -> $after)" }
Write-Host "ok   the front view grew upward ($before -> $after)"
Ui-ExpectNew '1 = 35' 'the dimension follows the block: 35 high'

Write-Host '--- 4. Saved and opened again'
Ui-Mark
Ui-Step 'back to the model'               { Ui-ClickLogged 'Drawing Close Drawing' }
Ui-ExpectNew 'Drawing closed' 'closed'
Ui-ExpectNew 'Ribbon tabs: SOLID, BUILD (shown BUILD)' 'back on the Build tab it was opened from'
Ui-Step 'save (Ctrl+S)'                   { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
if (-not (Select-String -Path $design -Pattern '"drawings"' -Quiet)) { Ui-Fail 'the design file has no drawings' }
Ui-StartApp 'drawings-again' @('--open', $design)
Ui-ExpectLog 'block.mitcad' 'the design opened again'
Ui-Mark
Ui-Step 'open Drawing1 (double-click)'    { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 2' 'the drawing opened'
Ui-ExpectNew 'Drawing sheet drawn: 4 views' 'its four views kept'

Write-Host '--- 5. Export a PDF'
$pdf = Join-Path $work 'drawing.pdf'
Ui-Mark
Ui-Step 'Export...'                         { Ui-ClickLogged 'Drawing Export' }
Ui-Step 'PDF... (Down, Enter)'              { Ui-Key 'Down'; Ui-Key 'Return' }
Ui-FocusDialog '^Export Drawing$'
Ui-Step 'the path'                        { Ui-Key 'ctrl+a'; Ui-Type $pdf; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew ': drawing Drawing1, 2 sheets' 'the PDF written, a page per sheet' 30
if (-not (Test-Path $pdf)) { Ui-Fail 'no PDF file' }
$head = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($pdf)[0..4])
if ($head -ne '%PDF-') { Ui-Fail "$pdf is no PDF" }
Write-Host "ok   $pdf is a PDF"
Write-Host '--- 5b. Export a DWG (mitcad#728)'
$dwg = Join-Path $work 'drawing.dwg'
Ui-Mark
Ui-Step 'Export...'                         { Ui-ClickLogged 'Drawing Export' }
Ui-Step 'DWG... (Down x4, Enter)'           { Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Return' }
Ui-FocusDialog '^Export Drawing$'
Ui-Step 'the path'                        { Ui-Key 'ctrl+a'; Ui-Type $dwg; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew ': drawing Drawing1, 2 sheets' 'the DWG written, a file per sheet' 30
if (-not (Test-Path $dwg)) { Ui-Fail 'no DWG file' }
$head = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($dwg)[0..5])
if ($head -ne 'AC1015') { Ui-Fail "$dwg is no R2000 DWG" }
if (-not (Test-Path (Join-Path $work 'drawing-2.dwg'))) { Ui-Fail 'no DWG file of the second sheet' }
Write-Host "ok   $dwg is an R2000 DWG, a file per sheet"
Ui-Mark
Settled 'Drawing Add Revision'
Ui-Step 'Add Revision'                    { Ui-ClickLogged 'Drawing Add Revision' }
Ui-ExpectNew 'Drawing revision dialog opened: Add Revision' 'its dialog'
Ui-Step 'its description'                 { Ui-FocusDialog '^Add Revision$'; Ui-Type 'First release'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Drawing revisions: A First release' 'the revision table has it'
Ui-ExpectNew 'REV=A' 'the title block shows the latest revision'
if (Ui-LogLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }

Write-Host '--- 6. A parts list and a balloon on an assembly'
# Component1 (30 x 20 x 5) at the origin and 40 along X, Component2
# (10 x 10 x 10) at y 30. The front view at 1:1 with its middle at 120, 120:
# x 0..70 at 85..155, z 0..10 at 115..125 on the sheet.
$assemblyScript = Join-Path $work 'assembly.json'
$assembly = Join-Path $work 'assembly.mitcad'
Set-Content -Encoding ascii $assemblyScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_component"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_component"}},
  {"cmd": "copy_occurrence", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $assemblyScript --save $assembly
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $assembly)) { Ui-Fail "mitcad-cli (the assembly): $($made -join ' / ')" }
Ui-StartApp 'drawings-assembly' @('--open', $assembly)
Ui-ExpectLog 'assembly.mitcad' 'the assembly opened'
Ui-Mark
Ui-Step 'Create Drawing (F9)'             { Ui-Key 'F9' }
Ui-ExpectNew 'Create Drawing dialog opened' 'its dialog'
Ui-Step 'OK'                              { Ui-FocusDialog '^Create Drawing$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'its sheet over the view'
Ui-ExpectNew 'Drawing group Tables at' "the tables' group on the ribbon"
Ui-ExpectNew 'Drawing sheet view ' 'the sheet placed'
Ui-ExpectNew 'Drawing tool: Base View' 'Base View on for the new drawing (mitcad#725)'
Ui-Mark
Ui-Step 'click the sheet at 120, 120'     { Sheet-Click 120 120 }
Ui-ExpectNew 'Drawing view 1 at' 'the front view drawn'
Ui-ExpectNew 'Drawing tool: Projected View' 'projected views follow'
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Ui-ExpectNew 'Drawing tool: selection' 'back to the selection'
Ui-Mark
Drawing-Tool 'Parts List' 'Tables'
Ui-ExpectNew 'Drawing tool: Parts List' 'the parts list tool is on'
# Its balloons off (on by default, mitcad#725): the balloon by hand below.
Ui-ExpectNew 'Drawing panel drawingOption_balloons at' 'its Balloons option'
Settled 'Drawing panel drawingOption_balloons'
Ui-Step 'Balloons off'                    { Ui-ClickLogged 'Drawing panel drawingOption_balloons' }
Ui-ExpectNew 'Drawing option balloons: off' 'no balloons'
Ui-Step 'the front view'                  { Ui-ClickLogged 'Drawing view 1' }
Ui-ExpectNew 'Drawing placing view' 'its place is clicked next'
Ui-Step 'where the list goes'             { Sheet-Click 150 190 }
Ui-ExpectNew '"type":"parts_list"' 'a parts list asked for'
Ui-ExpectNew '1 = 1 Component1 x2; 2 Component2 x1' 'sorted, numbered and counted'
Ui-Mark
Drawing-Tool 'Balloon' 'Tables'
Ui-ExpectNew 'Drawing tool: Balloon' 'the balloon tool is on'
Ui-Step "Component1:2's right side"       { Sheet-Click 154.8 117.5 }
Ui-ExpectNew 'Drawing picked line (1)' 'its edge picked'
Ui-ExpectNew 'Drawing placing view' 'its place is clicked next'
Ui-Step 'where the balloon goes'          { Sheet-Click 175 150 }
Ui-ExpectNew '"type":"balloon"' 'a balloon asked for'
Ui-ExpectNew '| 2 = 1' "the balloon shows Component1's number"
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
if (Ui-LogLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows drawings test'
