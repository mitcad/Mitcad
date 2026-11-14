# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp tools/cli core/model/src/build core/model-build core/model-api/src/build.rs
# check-all sources: app/MainWindowContextMenu.cpp tools/ui-windows-timber-lib.ps1
# Input-driven Windows UI test of the TIMBER group's fasteners (mitcad#497),
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# with the synthetic library of tools/ui-windows-timber-lib.ps1 (a wood
# screw, a ring nail and a pan-head connector screw in two sizes each, with
# heads to click on; mitcad#834: once part of
# tools/ui-windows-timber-test.ps1):
#   1. The first time the first countersunk wood screw, not the connector
#      screw whose name comes first (mitcad#516); Wall with a chosen
#      fastener (the picker's), size 5.0x90 and three at each junction;
#      Timber's Spacing on the wall's top plate with Fasteners on (the
#      fastener chosen in Wall, shared by the TIMBER group), each member
#      fastened; two of the wall's screws selected, the context menu's
#      BUILD > Swap Fastener (chosen with the keys) on them makes them
#      3.1x90 ring nails, one undo step. Wall's fastener rows lie below
#      the dock in session 0's low window: the panel scrolls to them
#      (Ui-PanelReveal).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-fasteners-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber-fasteners'),
  [string]$QtBin = '')

$TimberTest = 'UI Windows timber fasteners test'
. (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
Timber-Init

Write-Host '--- 1. Fasteners: a frame''s and a set''s, Swap Fastener (mitcad#497)'
Ui-StartApp 'timber-fasteners'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Wall (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Wall started' 'Wall started'
Expect-Inspect 'Wall' @('frame wall Wall 1', "fastener 2 ${Times} Wood screw head test at each junction, sized from the members") `
  ('the first time (fresh settings): two of the first countersunk wood screw, sized from the members, not the ' +
   'pan-head connector screw whose name comes first (mitcad#516)')
# The frame: three 5.0x90 screws with heads at each junction.
Choose-Fastener 'Wall' 'fastener' 'test-timber/wood-screw-head-test'
Ui-Mark
Ui-Step 'Size 5.0x90'                    { Wheel-Choice 'Wall' 'fastener_size' 2 }
Ui-Step 'three at each junction'         { Ui-TypeIn 'Panel Wall input fastener_count' '3' }
Ui-Step '1800 mm long'                   { Ui-TypeIn 'Panel Wall input frame_length' '1800' }
Ui-Step '600 mm high'                    { Ui-TypeIn 'Panel Wall input frame_height' '600' }
Expect-Inspect 'Wall' @("frame wall Wall 1, wall 1800 ${Times} 600 mm", "fastener 3 ${Times} Wood screw head test 5.0x90 at each junction") `
  'a chosen fastener and size, three at each junction'
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Wall 1 (wall, lead ' 'the wall placed with its screws' 60
$fastened = [int](Ui-LastMatch '^Placed frame Wall 1 \(wall, lead [^)]*\): \d+ member\(s\), (\d+) junction')
$screws = @(Heads-Since '9000')
if ($fastened -lt 1 -or $screws.Count -ne 3 * $fastened) {
  Ui-Fail "$($screws.Count) screw(s) at $fastened junction(s), not three at each"
}
Write-Host "ok   $fastened junction(s), three 5.0x90 screws each"
$seats = @(Ui-NewLines | Where-Object { $_ -match '^Timber fastener .*: head top ' })
if ($seats.Count -ne $screws.Count -or @($seats | Where-Object { $_ -notmatch 'head top 0 mm from entry face$' }).Count -gt 0) {
  Ui-Fail "the physical screw heads are not flush within 1e-6 mm: $($seats -join ' / ')"
}
Write-Host 'ok   every frame screw head is flush within 1e-6 mm'
$wall = @(New-Bodies | Where-Object { $screws -notcontains $_ })

# A set on the wall's top plate (its highest side face), fastened to it:
# Timber's Spacing, with the fastener chosen in Wall (shared by the TIMBER
# group's tools, mitcad#520).
Ui-Step 'Close Wall (Escape)'            { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Timber (F4)'                    { Ui-Key 'F4' }
Ui-ExpectNew 'Command Timber started' 'Timber started'
Ui-Step 'Mode: Spacing'                  { Ui-ClickLogged 'Panel Timber input how=spacing' }
Ui-Step '300 mm long'                    { Ui-TypeIn 'Panel Timber input length' '300' }
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Step "Timber's Place On again"        { Ui-ClickLogged 'Panel Timber input at' }
$top = @(Wall-Sides $wall) | Sort-Object { $_[3] } | Select-Object -First 1
if (-not $top) { Ui-Fail 'no side face of the wall to set the members on' }
Ui-Mark
Ui-Step 'the top plate''s top'           { Ui-Click $top[2] $top[3] }
Expect-Inspect 'Timber' @('place side', '; set of ', '; fasteners no') 'a set on the plate, no fasteners by default'
Ui-Mark
Ui-Step 'Fasteners on'                   { Ui-ClickLogged 'Panel Timber input set_fasteners' }
Expect-Inspect 'Timber' @('place side', '; set of ', "; fasteners 3 ${Times} Wood screw head test 5.0x90 at each junction") `
  'a set on the plate, fastened with the fastener chosen last'
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed Timber 48x98 test as ' 'the set placed' 60
$set = Ui-LastMatch '^Placed Timber 48x98 test as .*; a set of (\d+, \d+) junction'
$counts = "$set" -split ', '
if ($counts.Count -ne 2 -or [int]$counts[0] -lt 2 -or $counts[0] -ne $counts[1]) {
  Ui-Fail "not every member of the set fastened to the plate: $set"
}
Write-Host "ok   a set of $($counts[0]), each member fastened"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

# Swap Fastener: two of the frame's screws, the head highest in the view and
# the next highest at another junction. In session 0's low window a head is
# a few pixels wide and the three of a junction overlap, so a click on the
# middle of one there may pick another's corner: two clicks at one junction
# can take the same screw.
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Sync
$placed = @($screws | ForEach-Object {
    $body = $_
    $line = Ui-LogLines | Where-Object { $_ -match "^Body $([regex]::Escape($body)) in [O\d/]+ at \d+,\d+$" } | Select-Object -Last 1
    if ("$line" -match '^Body (.*) at (\d+),(\d+)$') { , @($Matches[1], [int]$Matches[2], [int]$Matches[3]) }
  } | Sort-Object { $_[2] })
$heads = @()
if ($placed.Count -gt 0) {
  $first = $placed[0]
  $other = $placed | Where-Object { [Math]::Abs($_[1] - $first[1]) + [Math]::Abs($_[2] - $first[2]) -gt 40 } | Select-Object -First 1
  if ($other) { $heads = @($first, $other) }
}
if ($heads.Count -ne 2) { Ui-Fail 'no two screw heads at two junctions in the view' }
Ui-Step 'click a screw'                  { Ui-Click $heads[0][1] $heads[0][2] }
Ui-Step 'Ctrl+click another'             { Ui-ClickWith 'ctrl' $heads[1][1] $heads[1][2] }
# The context menu is filled and logged: its Build has Swap Fastener for
# the screws. It stays open and takes the keys (as in
# tools/ui-windows-browser-open-test.ps1), so it is chosen with them, as on
# Linux with the mouse: Build is its third entry, Swap Fastener the first of
# its submenu.
Ui-Mark
Ui-Step 'right-click it'                 { Ui-Click $heads[1][1] $heads[1][2] 3 }
Ui-ExpectNew 'Context menu Build: ' 'the context menu''s Build'
$menu = Ui-LastMatch '^Context menu Build: (.*)$'
if (-not "$menu".StartsWith('Swap Fastener')) { Ui-Fail "Build has no Swap Fastener first: $menu" }
$entries = Ui-LastMatch '^Context menu: (.*)$'
if (-not "$entries".StartsWith('Repeat Timber | Solid | Build |')) { Ui-Fail "Build is not the menu's third entry: $entries" }
Write-Host 'ok   the context menu''s Build offers Swap Fastener'
Ui-Mark
Ui-Step 'BUILD > Swap Fastener'          { Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Right'; Ui-Key 'Return' }
Ui-ExpectNew 'Command Swap Fastener started' 'Swap Fastener started'
Ui-ExpectNew 'BUILD swap fastener starts with 2 fastener(s), test-timber|wood-screw-head-test' `
  'with the two screws selected, their own part'
Choose-Fastener 'Swap Fastener' 'part' 'test-timber/ring-nail-test'
Ui-Mark
Ui-Step 'Size 3.1x90'                    { Wheel-Choice 'Swap Fastener' 'fastener_size' 2 }
Expect-Inspect 'Swap Fastener' @("swap 2 fastener(s) (wood, 2 ${Times} Wood screw head test 5.0x90)", 'for Ring nail test 3.1x90') `
  'two screws to become 3.1x90 ring nails'
Ui-Mark
Ui-Step 'Swap (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Swapped 2 fastener(s) for Ring nail test 3.1x90 (3.1x90)' 'the two screws are ring nails' 30
Ui-ExpectNew 'head top 0 mm from entry face' 'the swapped nails are seated flush'
$seats = @(Ui-NewLines | Where-Object { $_ -match '^Timber fastener .*: head top ' })
if ($seats.Count -eq 0 -or @($seats | Where-Object { $_ -notmatch 'head top 0 mm from entry face$' }).Count -gt 0) {
  Ui-Fail "a swapped fastener stands proud: $($seats -join ' / ')"
}
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Swap 2 fasteners for Ring nail test 3.1x90' 'one undo step takes the swap back'

Ui-Finish $TimberTest
