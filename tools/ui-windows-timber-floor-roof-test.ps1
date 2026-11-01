# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp tools/cli core/model/src/build core/model-build core/model-api/src/build.rs
# check-all sources: tools/ui-windows-timber-lib.ps1
# Input-driven Windows UI test of the TIMBER group's Floor / Roof
# (mitcad#520) and of each frame a component of its own (mitcad#522), in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# with the synthetic library of tools/ui-windows-timber-lib.ps1
# (mitcad#834: once part of tools/ui-windows-timber-test.ps1):
#   1. Floor / Roof: a floor the first time, no wall's inputs, rim joists
#      all round, placed; Kind Roof waits for its two walls.
#   2. Each frame a component of its own (mitcad#522): three walls (Wall 2
#      and Wall 3 flush with Wall 1's ends), each logged as a component in
#      Component1, Component1 holding no joint; Timber Frame on a plate of
#      Wall 1 lays it out again in the same component; a gable roof on
#      Wall 2 and Wall 3 with ring nails (mitcad#521, the owner's case):
#      two at each junction, each at its junction's members, its head on
#      the entry face, their joints in Roof 1; the cut list by frame.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-floor-roof-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber-floor-roof'),
  [string]$QtBin = '')

$TimberTest = 'UI Windows timber floor and roof test'
. (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
Timber-Init

Write-Host '--- 1. Floor / Roof: a floor (mitcad#520)'
Ui-StartApp 'timber-floor'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-ExpectLog 'Ribbon button build.floor_roof at' 'the Floor / Roof button'
Ui-Mark
Ui-Step 'Floor / Roof (F5)'              { Ui-Key 'F5' }
Ui-ExpectNew 'Command Floor / Roof started' 'Floor / Roof started'
Ui-ExpectNew 'BUILD floor_roof kind floor' 'a floor the first time'
Ui-ExpectNew 'Panel Floor / Roof input frame_kind=roof at' 'the Kind''s Roof button'
$wallInputs = @(Ui-NewLines |
  Where-Object { $_ -match '^Panel Floor / Roof input (frame_height|frame_corner|frame_top) at \d+,\d+$' })
if ($wallInputs.Count -gt 0) { Ui-Fail "Floor / Roof shows a wall's inputs: $($wallInputs -join ' / ')" }
Expect-Inspect 'Floor / Roof' @("frame floor Floor 1, floor 4000 ${Times} 3000 mm", '4 rim joist') `
  'a floor from the origin: rim joists all round, joists between them'
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Floor 1 (floor, lead ' 'the floor placed' 60
Ui-Mark
Ui-Step 'Kind: Roof'                     { Ui-ClickLogged 'Panel Floor / Roof input frame_kind=roof' }
Ui-ExpectNew 'View hint Floor / Roof: Click a member of each of the two walls the roof stands on' `
  'a roof waits for its two walls'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Write-Host '--- 2. Each frame a component of its own; a gable roof''s ring nails at its junctions (mitcad#522, mitcad#521)'
First-Wall 'timber-roof' 'butt'
$studs1 = $script:Studs1
Ui-ExpectNew 'Frame component Wall 1 (' 'Wall 1 a component of its own (mitcad#522)'
$frame1 = Ui-LastMatch '^Frame component Wall 1 \((C\d+)\) in Component1$'
if (-not $frame1) { Ui-Fail 'Wall 1 is no component of its own in Component1' }
Ui-ExpectNew 'Joints by component: Component1 0' 'no joint of Wall 1 in Component1'
$plates1 = @(Largest-Bodies)
Start-Picks
Wall-From 'Wall 2' (Stud-Face $studs1 'first') '3000' "from Wall 1's side, flush with end A (butt)" 1
Ui-ExpectNew 'Frame component Wall 2 (' 'Wall 2 a component of its own'
$studs2 = @(Stud-Bodies)
Start-Picks
Wall-From 'Wall 3' (Stud-Face $studs1 'last') '3000' "from Wall 1's side, flush with end B (butt)" 1
Ui-ExpectNew 'Frame component Wall 3 (' 'Wall 3 a component of its own'
$studs3 = @(Stud-Bodies)
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
# Timber Frame (MODIFY) on a plate of Wall 1 lays it out again in place, in
# the same component.
Ui-Mark
Ui-Step 'Timber Frame (F12)'             { Ui-Key 'F12' }
Ui-ExpectNew 'Command Timber Frame started' 'Timber Frame started'
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Step 'its Member'                     { Ui-ClickLogged 'Panel Timber Frame input part' }
Ui-Sync
Ui-Mark
# Of Wall 1's plates the one a click picks now (the bottom plate may be out
# of sight).
$plate1 = Pickable $plates1
if (-not $plate1) { Ui-Fail "no plate of Wall 1 ($($plates1 -join ', ')) to pick" }
Ui-Step 'a plate of Wall 1'              { Ui-ClickPick "$plate1/" }
Expect-Inspect 'Timber Frame' @('frame Wall 1 laid out again: ') "Wall 1's values in Timber Frame"
Ui-Mark
Ui-Step 'Spacing 400'                    { Ui-TypeIn 'Panel Timber Frame input frame_spacing' '400 mm' }
Expect-Inspect 'Timber Frame' @('frame Wall 1 laid out again: ') 'the studs at 400'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Timber frame Wall 1: ' 'Wall 1 laid out again' 60
Ui-ExpectNew "Frame component Wall 1 ($frame1) in Component1" 'in its own component, the same one'
Ui-ExpectNew 'Joints by component: Component1 0' 'still no joint in Component1'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
# A gable roof on Wall 2 and Wall 3 with ring nails (the owner's case of
# mitcad#521), two at each junction.
Ui-Mark
Ui-Step 'Floor / Roof (F5)'              { Ui-Key 'F5' }
Ui-ExpectNew 'Command Floor / Roof started' 'Floor / Roof started'
Ui-Step 'Kind: Roof'                     { Ui-ClickLogged 'Panel Floor / Roof input frame_kind=roof' }
Ui-ExpectNew 'View hint Floor / Roof: Click a member of each of the two walls the roof stands on' `
  'a roof waits for its two walls'
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Step 'its Walls'                      { Ui-ClickLogged 'Panel Floor / Roof input frame_walls' }
Ui-Sync
$at2 = Stud-Face $studs2 'middle'
$at3 = Stud-Face $studs3 'middle'
Ui-Mark
Ui-Step 'a stud of Wall 2'               { Ui-Click $at2[0] $at2[1] }
Ui-Step 'a stud of Wall 3'               { Ui-Click $at3[0] $at3[1] }
Expect-Inspect 'Floor / Roof' @('frame roof Roof 1, gable roof', 'rafter', 'ridge board', 'fasteners yes') `
  'a gable roof on Wall 2 and Wall 3, with fasteners'
Choose-Fastener 'Floor / Roof' 'fastener' 'test-timber/ring-nail-test'
Ui-Mark
Ui-Step 'two at each junction'           { Ui-TypeIn 'Panel Floor / Roof input fastener_count' '2' }
Expect-Inspect 'Floor / Roof' @('frame roof Roof 1, gable roof', "fastener 2 ${Times} Ring nail test") `
  'two ring nails at each junction'
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Roof 1 (roof, lead ' 'the roof placed' 60
$fastened = [int](Ui-LastMatch '^Placed frame Roof 1 \(roof, lead [^)]*\): \d+ member\(s\), (\d+) junction')
if ($fastened -lt 1) { Ui-Fail 'no junction of the roof fastened' }
Ui-ExpectNew 'Frame component Roof 1 (' 'Roof 1 a component of its own'
Ui-ExpectNew 'Joints by component: Component1 0, ' 'Component1 holds no joint, the roof its nails'''
$joints = Ui-NewLines | Where-Object { $_.StartsWith('Joints by component: ') } |
  Select-Object -Last 1
if (-not "$joints".Contains("Roof 1 $(2 * $fastened)")) { Ui-Fail "the roof's joints not in Roof 1: $joints" }
Write-Host "ok   $joints"
# No nail beside the roof: each one at its junction, in its members (the
# test nails are heads only: through the member entered), its head on the
# entry face.
$nails = @(Heads-Since '4000')
if ($nails.Count -ne 2 * $fastened) { Ui-Fail "$($nails.Count) nail(s) at $fastened junction(s), not two each" }
Ui-ExpectNew 'Fastener shaft ' 'the nails measured'
$shafts = @(Ui-NewLines | Where-Object { $_.StartsWith('Fastener shaft ') })
$off = @($shafts | Where-Object { $_ -match ' through 0 of ' })
if ($shafts.Count -ne $nails.Count -or $off.Count -gt 0) {
  Ui-Fail "a nail beside its junction's members: $($shafts -join ' / ')"
}
$seats = @(Ui-NewLines | Where-Object { $_ -match '^Timber fastener .*: head top ' })
if (@($seats | Where-Object { $_ -notmatch 'head top 0 mm from entry face$' }).Count -gt 0) {
  Ui-Fail "a nail's head off the entry face: $($seats -join ' / ')"
}
Write-Host "ok   $($nails.Count) nails at their junctions, flush"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
# The cut list by frame (mitcad#522).
Ui-Mark
Ui-Step 'Cut List (F9)'                  { Ui-Key 'F9' }
Ui-FocusDialog '^Cut List$'
Ui-ExpectNew 'Cut list row: ' "the cut list's rows"
foreach ($name in @('Wall 1', 'Wall 2', 'Wall 3', 'Roof 1')) {
  $rows = @(Ui-NewLines | Where-Object { $_.StartsWith('Cut list row: ') -and $_.EndsWith(", frame $name") })
  if ($rows.Count -lt 1) { Ui-Fail "no cut list row of $name" }
}
Write-Host 'ok   the cut list by frame: Wall 1, Wall 2, Wall 3, Roof 1'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Ui-Finish $TimberTest
