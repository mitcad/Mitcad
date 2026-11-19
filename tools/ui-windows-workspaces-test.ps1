# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/TitleBar.cpp app/framework/FloatingLayout.cpp app/drawings/MainWindowDrawings.cpp app/MainWindow.cpp app/framework/TestDriver.cpp app/files/ProjectIndicator.cpp app/mcp/AgentIndicator.cpp tools/cli tools/ui-scripts/workspaces.mitcad-ui tools/ui-scripts/workspaces.json app/framework/MenuBarReveal.cpp app/DocumentFrame.cpp app/drawings/DrawingWorkspace.cpp app/resources/layout/default.json app/framework/QuickAccess.cpp app/framework/QuickAccessGroup.cpp app/drawings/DrawingsPanel.cpp app/browser/BrowserController.cpp app/MainWindowRender.cpp app/render/RenderWorkspace.cpp app/framework/GlassCard.cpp
# The toolbar panel's workspace selector (mitcad#1990) in the build VM, as
# tools/ui-workspaces-test.sh on Linux: the shared steps
# tools/ui-scripts/workspaces.mitcad-ui in the Floating layout on a box
# with one drawing (tools/ui-scripts/workspaces.json), in a 1024 x 768
# window: no menu bar, the menu button in the top bar, no quick-access group there
# (no commands by default); Design's tabs alone (Make's in its own
# workspace), the Start page from Home without the panel and left by the
# design's tab, then Design -> Make (its three tabs alone, each shown) ->
# Render (its tab alone) -> Drawing (the drawing opens) -> Design (it
# closes) from the selector's list, then the groups that do not fit an
# 820 px window even collapsed in the row's more menu. The pictures of the
# Start page, the Solid tab, the open list, the Make workspace's three
# tabs, the Render and the Drawing workspaces and the narrow window are
# kept in -Out's pictures folder.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-workspaces-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-workspaces'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$pictures = Join-Path $script:UiOut 'pictures'
New-Item -ItemType Directory -Force $pictures | Out-Null
$design = Join-Path $work 'workspaces.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/workspaces.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $pictures
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/workspaces.mitcad-ui'
Ui-StartApp 'workspaces' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'workspaces driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "workspaces driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Ribbon panel hidden (Start)' 'the Start page without the toolbar panel'
Ui-ExpectLog 'Ribbon workspace: MAKE (tabs MAKE_PRINT, MAKE_LASER, MAKE_CNC)' 'Design -> Make: its three tabs alone'
Ui-ExpectLog "Ribbon workspace entry MAKE tabs: 3D Print $([char]0x00B7) Laser $([char]0x00B7) CNC" "Make's entry names its tabs"
Ui-ExpectLog 'Ribbon workspace: RENDER (tabs RENDER)' 'Make -> Render: the Render tab alone'
Ui-ExpectLog 'Workspace DRAWING: opened D1' "Render -> Drawing: the design's drawing opened"
Ui-ExpectLog 'Ribbon workspace: DRAWING (tabs DRAWING)' 'the Drawing tab alone'
Ui-ExpectLog 'Drawings card: Drawing1 (open, sheet 1 of 1)' "the Drawings card in place of the Browser's"
Ui-ExpectLog 'Renamed Drawing1 to Plan' "the Drawings card's Rename"
# The Drawings card in a column of its own (mitcad#1990): the drawing's bar
# and sheet start right of it, its top level with the bar's.
$column = Ui-LastMatch '^(Drawing column at .* beside card .*)$'
if ($null -eq $column) { Ui-Fail "the drawing's bar and sheet not beside the Drawings card" }
if ($column -notmatch '^Drawing column at (-?\d+),(-?\d+) beside card (-?\d+),(-?\d+) (\d+)x\d+$') {
  Ui-Fail "no Drawings card in '$column'"
}
if ([int]$Matches[1] -lt [int]$Matches[3] + [int]$Matches[5]) {
  Ui-Fail "the drawing's bar and sheet start under the Drawings card: '$column'"
}
if ([int]$Matches[2] -ne [int]$Matches[4]) { Ui-Fail "the Drawings card's top is not level with the drawing's bar: '$column'" }
Write-Host "ok   the Drawings card in a column of its own, level with the drawing's bar"
# One toolbar panel height in every workspace (mitcad#1990): a switch never
# moves the cards or the view below.
$heights = @{}
foreach ($space in @('DESIGN', 'MAKE', 'RENDER', 'DRAWING')) {
  $height = Ui-LastMatch "^Ribbon panel height ${space}: (\d+)$"
  if ($null -eq $height) { Ui-Fail "no toolbar panel height logged in $space" }
  $heights[$space] = [int]$height
}
if (@($heights.Values | Sort-Object -Unique).Count -ne 1) {
  Ui-Fail ("the toolbar panel's height differs between the workspaces: " + (($heights.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' '))
}
Write-Host 'ok   one toolbar panel height in every workspace'
# The + right after the workspace's tabs (mitcad#1990): the hidden tabs of
# the other workspaces take no room in the row. Render has no + (its stages
# stand in the row in place of the tabs).
foreach ($space in @('DESIGN', 'MAKE', 'DRAWING')) {
  $gap = Ui-LastMatch "^(Ribbon tabs gap ${space}: .*)$"
  if ($null -eq $gap) { Ui-Fail "no tab row logged in $space" }
  if ($gap -notmatch 'last tab ends at x (-?\d+), \+ starts at x (-?\d+)$') { Ui-Fail "no places in the tab row line '$gap'" }
  $apart = [int]$Matches[2] - [int]$Matches[1]
  if ($apart -lt 0 -or $apart -gt 16) { Ui-Fail "the + is $apart px from the last tab in ${space}: '$gap'" }
}
Write-Host 'ok   the + right after the last tab in Design, Make and Drawing'
# One bottom bar in every workspace (mitcad#1990): Render's Shots bar where
# Design's timeline bar is, as wide and as high (the timeline's last place
# before Render, in the same window size).
$timeline = $null
$shots = $null
foreach ($line in @(Ui-LogLines)) {
  if ($line -notmatch '^Floating cards: ') { continue }
  foreach ($part in ($line -split '; ')) {
    $f = $part -split ' '
    if ($f[0] -eq 'timelineDock' -and $null -eq $shots) { $timeline = "$($f[1]) $($f[2])" }
    if ($f[0] -eq 'renderBarPanel') { $shots = "$($f[1]) $($f[2])" }
  }
}
if ($null -eq $timeline) { Ui-Fail 'no timeline bar logged in Design' }
if ($null -eq $shots) { Ui-Fail 'no Shots bar logged in Render' }
if ($timeline -ne $shots) { Ui-Fail "the Shots bar is not where the timeline bar is: timeline $timeline, Shots $shots" }
Write-Host "ok   one bottom bar in Design and Render: $timeline"
Ui-ExpectLog 'Workspace DESIGN: the drawing closed' 'Drawing -> Design: the drawing closed'
Ui-ExpectLog 'Ribbon more SOLID: none' 'the more menu gone in a wide window again'
Ui-ExpectLog 'TestDriver: done' 'the workspace selector'
# The project and assistant indicators in the title row: compact icon
# buttons with a caret, their text in the tooltip (mitcad#1990).
$cards = Ui-LastMatch '^(Floating cards: .*)$'
foreach ($part in @('titleIndicator', 'titleAgent')) {
  $entry = @(($cards -split '; ') | Where-Object { $_ -match "^$part " })
  if ($entry.Count -eq 0) { Ui-Fail "no $part in the title row: '$cards'" }
  $width = [int]((($entry[0] -split ' ')[2] -split 'x')[0])
  if ($width -gt 48) { Ui-Fail "$part is $width px wide, not a compact icon button: '$cards'" }
}
Write-Host 'ok   the project and assistant indicators are compact icon buttons'
# No menu bar row: the window's menu bar hidden, the title row at the
# window's top.
Ui-ExpectLog 'Menu bar hidden: Floating' "the window's menu bar hidden"
if ($cards -notmatch '; titleRow 0,0 ') { Ui-Fail "the title row is not at the window's top: '$cards'" }
Write-Host 'ok   no menu bar above the title row'
# No quick-access group in the top bar: it has no commands by default and
# is hidden while empty (mitcad#1990).
Ui-ExpectLog 'Quick access group hidden: no commands' 'the empty quick-access group hidden'
Ui-QuickAccessPlace 'the title row' $cards -None
foreach ($picture in @('start', 'solid', 'list', 'make-print', 'make-laser', 'make-cnc', 'render', 'drawing', 'narrow')) {
  $path = Join-Path $pictures "workspaces-$picture.png"
  if (-not (Test-Path $path) -or (Get-Item $path).Length -eq 0) { Ui-Fail "no picture workspaces-$picture.png in $pictures" }
}
Write-Host "ok   the pictures in $pictures"
Ui-StopApp
Write-Host 'PASS: the workspace selector switches Design, Make, Render and Drawing'
