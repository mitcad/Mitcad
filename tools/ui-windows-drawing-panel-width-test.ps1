# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser app/framework/FloatingLayout.cpp app/MainWindow.cpp tools/cli
# Input-driven Windows UI test of the drawing workspace's panel width
# (mitcad#1692, GitHub #266) in the build VM (tools/ui-windows-lib.ps1 says
# how the input is posted), as tools/ui-drawing-panel-width-test.sh does on
# Linux, on Plate (a 40 x 30 x 5 body) placed twice, with a drawing whose
# base view draws both, that mitcad-cli makes:
#   1. Floating, the test's own settings: the panel is 230 px wide; the base
#      view's Shows spans it under its label, and Update from Model's whole
#      text fits its button.
#   2. The panel's edge dragged toward the sheet: the panel and Shows are
#      wider by the same amount, and the width is remembered.
#   3. Close Drawing, the drawing opened again: the panel keeps its width.
#   4. Docked, at the next start: the panel has the width again; its edge
#      dragged wider still is remembered too.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-panel-width-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-panel-width'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

$modelScript = Join-Path $work 'plates.json'
$design = Join-Path $work 'plates.mitcad'
Set-Content -Encoding ascii $modelScript ((@'
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "copy_occurrence", "occurrence": "Plate:1", "transform": {"translation": [50, 0, 0]}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front", "at": [150, 150], "scale": "1:1"}
]
'@) -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Write-Host 'ok   a design with a drawing made'

# Settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait.
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

# Last-Width "Drawing panel <what>": the number after "<what> width " in the
# log's last such line since the mark ($null for none).
function Last-Width([string]$What) {
  $found = $null
  $pattern = '^' + [regex]::Escape($What) + ' width (\d+)'
  foreach ($line in @(Ui-NewLines)) {
    $match = [regex]::Match($line, $pattern)
    if ($match.Success) { $found = [int]$match.Groups[1].Value }
  }
  return $found
}

# Wait-Width "Drawing panel <what>" width why: until the last such line
# since the mark has that width.
function Wait-Width([string]$What, [int]$Width, [string]$Why) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $got = Last-Width $What
    if ($got -eq $Width) { Write-Host "ok   $Why ($What width $Width)"; return }
    Ui-Sync
    Start-Sleep -Milliseconds 200
  } while ($clock.ElapsedMilliseconds -lt 10000)
  Ui-Fail "${Why}: $What is $got px wide, not $Width"
}

# Remembered: the width of the last "Drawing panel width N remembered" line
# since the mark (the edge let go), waited for; $null when none came.
function Remembered {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $found = $null
    foreach ($line in @(Ui-NewLines)) {
      $match = [regex]::Match($line, '^Drawing panel width (\d+) remembered$')
      if ($match.Success) { $found = [int]$match.Groups[1].Value }
    }
    if ($null -ne $found) { Write-Host 'ok   the width is remembered'; return $found }
    Start-Sleep -Milliseconds 200
  } while ($clock.ElapsedMilliseconds -lt 10000)
  return $null
}

# Drag-Edge dx: the panel's edge dragged dx pixels across.
function Drag-Edge([int]$Dx) {
  $at = Ui-LoggedAt 'Drawing panel edge'
  Ui-Drag $at[0] $at[1] ($at[0] + $Dx) $at[1]
  Ui-Sync
}

function Select-View {
  Ui-Mark
  Ui-Step 'select the front view'         { Ui-ClickLogged 'Drawing view 1' }
  Ui-ExpectNew 'Drawing view 1 selected' 'the front view selected'
  Ui-ExpectNew 'Drawing panel drawingViewShows at' 'its Shows in the panel'
  Ui-ExpectNew 'Drawing panel drawingViewUpdate at' 'Update from Model in the panel'
  Settled 'Drawing panel drawingViewUpdate'
}

# Button-Fits why: Update from Model's text fits its button.
function Button-Fits([string]$Why) {
  $width = $null
  $needs = $null
  foreach ($line in @(Ui-NewLines)) {
    $match = [regex]::Match($line, '^Drawing panel drawingViewUpdate width (\d+), its text needs (\d+)$')
    if ($match.Success) { $width = [int]$match.Groups[1].Value; $needs = [int]$match.Groups[2].Value }
  }
  if ($null -eq $width) { Ui-Fail 'the button''s width was not logged' }
  if ($needs -gt $width) { Ui-Fail "Update from Model needs $needs px, its button is $width px: $Why" }
  Write-Host "ok   Update from Model's text fits ($needs of $width px): $Why"
}

function Close-Drawing {
  Ui-Mark
  Settled 'Drawing Close Drawing'
  Ui-Step 'Close Drawing'                 { Ui-ClickLogged 'Drawing Close Drawing' }
  Ui-ExpectNew 'Drawing closed' 'the 3D view again'
}

function Open-Drawing {
  Ui-Mark
  Ui-Step 'open Drawing1 (double-click)'  { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
  Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'the drawing opened'
  Ui-ExpectNew 'Drawing sheet drawn: ' 'its sheet drawn'
}

Write-Host '--- 1. Floating: the panel at its narrowest'
Ui-StartApp 'drawing-panel-width' @('--chrome=floating', '--open', $design)
Ui-ExpectLog 'plates.mitcad' 'the design opened'
Open-Drawing
Select-View
Wait-Width 'Drawing panel' 230 'the panel at its narrowest at first'
$shows = Last-Width 'Drawing panel drawingViewShows'
if ($null -eq $shows) { Ui-Fail 'the Shows list''s width was not logged' }
if ($shows -lt 200) { Ui-Fail "Shows is $shows px wide in a 230 px panel: not across the panel" }
Write-Host "ok   Shows spans the panel ($shows px)"
Button-Fits 'at the narrowest panel'

Write-Host '--- 2. The panel''s edge dragged toward the sheet'
Ui-Mark
Ui-Step 'drag the panel''s edge to the left' { Drag-Edge -150 }
$wide = Remembered
if ($null -eq $wide -or $wide -lt 340) { Ui-Fail "the panel is $wide px wide after the drag" }
Write-Host "ok   the panel is $wide px wide"
Wait-Width 'Drawing panel' $wide 'the panel laid out at its new width'
Wait-Width 'Drawing panel drawingViewShows' ($shows + $wide - 230) 'Shows wider by the same amount'
Button-Fits 'in the wider panel'

Write-Host '--- 3. The drawing opened again: the width kept'
Close-Drawing
Open-Drawing
Select-View
Wait-Width 'Drawing panel' $wide 'the panel keeps its width'
Wait-Width 'Drawing panel drawingViewShows' ($shows + $wide - 230) 'Shows keeps its width'

Write-Host '--- 4. Docked, at the next start: the width again, and wider still'
Ui-StartApp 'drawing-panel-width-docked' @('--open', $design)
Ui-ExpectLog 'plates.mitcad' 'the design opened'
Open-Drawing
Ui-ExpectLog "Drawing panel width $wide (remembered)" 'the width read from the settings'
Select-View
Wait-Width 'Drawing panel' $wide 'the panel has its width again'
Wait-Width 'Drawing panel drawingViewShows' ($shows + $wide - 230) 'Shows has its width again'
Ui-Mark
Ui-Step 'drag the panel''s edge to the left' { Drag-Edge -60 }
$wider = Remembered
if ($null -eq $wider -or $wider -lt ($wide + 40)) { Ui-Fail "the docked panel is $wider px wide (was $wide)" }
Write-Host "ok   the docked panel is $wider px wide (was $wide)"
Wait-Width 'Drawing panel drawingViewShows' ($shows + $wider - 230) 'Shows wider by the same amount, docked'

if (Ui-LogLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }
Ui-Finish 'UI Windows drawing panel width test'
