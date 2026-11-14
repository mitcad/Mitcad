# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchItemList.cpp app/sketch/SketchItemList.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchReveal.cpp
# check-all sources: app/sketch/SketchModel.cpp app/sketch/DrawTools.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/order.rs core/sketch/src/steps.rs core/model-api/src/sketch.rs
# check-all sources: core/model-api/src/dispatch.rs core/model/src/document.rs
# check-all sources: tools/ui-scripts/sketch-item-list.mitcad-ui tools/ui-scripts/sketch-item-list.json
# check-all sources: app/browser/FeatureStatus.cpp app/framework/Icons.cpp app/framework/FloatingLayout.cpp
# Sketch mode's list of the sketch's work steps in the timeline's place
# (mitcad#1750, after mitcad#1492's list of items) in the build VM, as
# tools/ui-sketch-item-list-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-item-list.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-item-list.json): a Project of a face's
# outline, a Rectangle and a dimension as three cells, icons only; the
# Project cell's hover list, its low rows under the pointer, a row's
# highlight and a row's click, the Project cell then drawn selected with its
# icon in its own colors; the dimension's value editor from its cell; a
# refused Parallel's error badge on the Rectangle cell; the Rectangle's
# items selected from its cell and deleted with it; a sketch saved without
# steps shown one cell per item, its inactive Horizontal's cell dimmed; and
# the timeline back after Finish Sketch, in the Docked layout and in the
# Floating one, where the hover list is over the message band's card
# (mitcad#1763). In both the icons are in the bar's middle (mitcad#1760);
# the Floating card is a row of cells high in sketch mode, the Docked panel
# keeps the timeline's height, and the view never changes its size.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-item-list-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-item-list'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'items.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-item-list.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
# The bar's places (mitcad#1760), as tools/ui-sketch-item-list-test.sh checks
# them: each "Sketch step <key> icon at x,y wxh" line against the "Sketch
# steps bar at x,y wxh" before it, the icon in the bar's middle (equal room
# above and below within 1 px) and the bar no more than $Most px taller than
# the icon: Floating 14 (the cell's 3 px around it and the card's 4 px
# margins), Docked 24 (the timeline's panel, with its marker's band).
function Check-Bar([string]$Layout, [int]$Most) {
  $top = $null
  $high = 0
  $icons = 0
  foreach ($line in Ui-LogLines) {
    if ($line -match '^Sketch steps bar at -?\d+,(-?\d+) \d+x(\d+)') {
      $top = [int]$Matches[1]
      $high = [int]$Matches[2]
    } elseif ($null -ne $top -and $line -match '^Sketch step (\S+) icon at -?\d+,(-?\d+) \d+x(\d+)') {
      $above = [int]$Matches[2] - $top
      $below = $top + $high - [int]$Matches[2] - [int]$Matches[3]
      if ([Math]::Abs($above - $below) -gt 1) {
        Ui-Fail "${Layout}: icon $($Matches[1]) off the middle: $above px above, $below below"
      }
      if ($high - [int]$Matches[3] -gt $Most) { Ui-Fail "${Layout}: bar $high px high for a $($Matches[3]) px icon" }
      $icons++
    }
  }
  if ($icons -eq 0) { Ui-Fail "${Layout}: no icon places logged" }
  Write-Host "ok   ${Layout}: the icons in the middle of the bar, at most $Most px taller than they are"
}

# The view's size: the same all along, in sketch mode too (a view that
# changed its height for a sketch drew the design at another size after it).
# From the size it had when the first sketch's edit began: in session 0 the
# window's client area shrinks once at the start, when Windows 11 puts its
# frame on the window ("Window frame: ..."), before any sketch (mitcad#1787).
function Check-View([string]$Layout) {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -like 'View area *') { $start = $i }
    if ($lines[$i] -like 'Editing sketch *') { break }
  }
  $areas = @($lines[$start..($lines.Count - 1)] | Where-Object { $_ -like 'View area *' } | Sort-Object -Unique)
  if ($areas.Count -ne 1) { Ui-Fail "${Layout}: the view changed its size: $($areas -join '; ')" }
  Write-Host "ok   ${Layout}: the view kept its size"
}

function Run-App([string]$Layout, [string]$Script) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Ui-StartApp "sketch-item-list-$Layout" @('--ui=qt', '--no-mcp', "--chrome=$Layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "sketch step list driver timed out ($Layout)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "sketch step list driver failed ($Layout, exit code $exitCode)"
  }
}

# The whole script in each layout (mitcad#1763: in the Floating one too);
# the bar at most this many px taller than an icon (mitcad#1760): the
# Floating card a row of cells and its margins, the Docked panel the
# timeline's height.
$limit = @{ 'docked' = 24; 'floating' = 14 }
foreach ($layout in @('docked', 'floating')) {
  Run-App $layout 'sketch-item-list.mitcad-ui'
  Ui-ExpectLog "Sketch item list in the timeline's place" "${layout}: the step list in the timeline's place in sketch mode"
  Ui-ExpectLog ' project x' "${layout}: the face projected as one Project step"
  Ui-ExpectLog ' rectangle x' "${layout}: the rectangle as one step"
  Ui-ExpectLog ' add_dimension x1' "${layout}: the dimension as one step"
  Ui-ExpectLog ' hover list: Project: ' "${layout}: the Project step's hover list"
  Ui-ExpectLog ' hovered' "${layout}: the hover list's rows under the pointer"
  Ui-ExpectLog ' clicked' "${layout}: an item selected from its row"
  Ui-ExpectLog ' drawn: selected, icon normal' "${layout}: a selected cell's icon in its own colors"
  Ui-ExpectLog ' [error]' "${layout}: the conflict badge on its step"
  Ui-ExpectLog 'Sketch steps: p1 point, p2 point, c3 line, k4 horizontal inactive' "${layout}: a sketch without steps: one cell per item"
  Ui-ExpectLog 'Sketch step k4 drawn: dimmed, icon disabled' "${layout}: an inactive item's cell dimmed"
  Ui-ExpectLog 'Timeline back in its place' "${layout}: the timeline back after the sketch"
  Ui-ExpectLog 'TestDriver: done' "${layout}: the steps listed, hovered, selected, deleted and edited"
  Check-Bar $layout ($limit[$layout])
  Check-View $layout
}
Ui-StopApp
Write-Host 'PASS: sketch step list'
