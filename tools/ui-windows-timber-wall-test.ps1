# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp tools/cli core/model/src/build core/model-build core/model-api/src/build.rs
# check-all sources: tools/ui-windows-timber-lib.ps1
# Input-driven Windows UI test of the BUILD tab's TIMBER group's Wall
# (mitcad#520; mitcad#493, mitcad#510), in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), with the
# synthetic library of tools/ui-windows-timber-lib.ps1 (mitcad#834: once
# part of tools/ui-windows-timber-test.ps1):
#   1. Walls: a wall along X, Corner Butt (the default); after Place
#      nothing previewed from the origin on top of it and Enter places no
#      second wall there, the view asking where the next wall starts
#      (mitcad#513); a wall started by a click on its end stud's side, a
#      few centimetres from the corner, comes flush with end A (mitcad#514;
#      a backing stud more in the first wall), an Offset typed moving the
#      preview once and 0 bringing it back; one by a click on its end B's
#      face (the view from the right) starts from that end (a butt corner).
#      Then, in a new design, Corner Lapped plates: a click on the side
#      near end B makes an L corner (a junction stud more in the first
#      wall), one mid-side a T; each with a typed length, the panel naming
#      where it starts and the corner, each placed with the members
#      previewed; the next Wall start remembers Lapped plates. Then, in a
#      new design (mitcad#515), two walls flush with the first one's ends
#      and a wall started on the first of them towards the other: no End
#      clicked, it ends butting the other's side.
#   2. A wall with Wall, then Timber's Two points from a stud's side to the
#      next stud's (mitcad#509): the sides are not one plane, so the member
#      is free between the points (not its end moved onto the start's
#      plane, a member a few millimetres long), as long as the studs'
#      distance, its end cut back to the next stud's face.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-wall-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber-wall'),
  [string]$QtBin = '')

$TimberTest = 'UI Windows timber wall test'
. (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
Timber-Init

Write-Host "--- 1. Walls: butting from a wall's side and its end, Lapped plates an L and a T (mitcad#493, mitcad#510)"
First-Wall 'timber-walls' 'butt'
$wall1 = $script:Wall1
# After Place no ghost (mitcad#513): the panel keeps its values, Start and
# End are empty, nothing is previewed from the origin on top of Wall 1 and
# Enter places no second wall there; a click on a start previews the next
# wall there (Wall-From below).
Ui-ExpectNew 'View hint Wall: Click where the next wall starts' 'the view asks where the next wall starts'
Expect-Inspect 'Wall' @('failed: Click where the next wall starts.') 'the panel waits for the next wall''s start'
No-Ghost 'Wall' 'frame wall ' 'no wall previewed at the origin on top of Wall 1 after Place'
Ui-Mark
Ui-Step 'Enter without a click'          { Ui-Key 'Return' }
Ui-ExpectNew 'Wall: OK refused' 'Enter places no second wall at the origin'
No-Ghost 'Wall' 'frame wall ' 'still nothing previewed'
# The owner's case (mitcad#514): a click on its end stud's front face, a
# few centimetres from the corner (+X to the right in the view), comes
# flush with end A, butting, no L: its outer face in the plane of the end
# face. An Offset typed moves the preview once (nothing follows the
# mouse), 0 brings it back flush; placed, a backing stud more in Wall 1.
Start-Picks
$corner = Stud-Face $script:Studs1 'first'
Ui-Mark
Ui-Step "the start near Wall 1's corner" { Ui-Click $corner[0] $corner[1] }
Ui-ExpectNew 'BUILD frame start click: From End A, Offset 0 mm (side)' 'the click fills From and Offset: flush'
Ui-ExpectNew 'View hint Wall: Click where the wall ends, or press Enter: Length square to the wall clicked' `
  'the end next, else Length square to the wall'
Ui-Mark
Ui-Step '3000 mm long'                   { Ui-TypeIn 'Panel Wall input frame_length' '3000' }
Expect-Inspect 'Wall' @("frame wall Wall 2, wall 3000 ${Times} 2400 mm", "; Wall 2: from Wall 1's side, flush with end A (butt)",
  '; corner butt', '; length typed') 'Wall 2 previewed flush with end A, the length typed'
Ui-Mark
Ui-Step 'Offset 600'                     { Ui-TypeIn 'Panel Wall input frame_offset' '600' }
Ui-ExpectNew 'BUILD frame offset typed: from End A' 'the offset typed'
Expect-Inspect 'Wall' @("; Wall 2: from Wall 1's side, 600 mm from end A (butt)") 'the preview 600 mm from end A'
Ui-Sync
Ui-Mark
Ui-Step 'the mouse over the wall'        { [UiInput]::Move($script:UiWindow, ($corner[0] + 40), $corner[1], 0x04) }
Ui-Step 'the mouse off the parts'        { Off-Parts (View-Area) }
No-Inspect 'Wall' 'the preview stays where the offset put it while the mouse moves'
Ui-Mark
Ui-Step 'Offset 0'                       { Ui-TypeIn 'Panel Wall input frame_offset' '0' }
Expect-Inspect 'Wall' @("; Wall 2: from Wall 1's side, flush with end A (butt)") 'Offset 0: flush with end A again'
$line = Ui-NewLines |
  Where-Object { $_.StartsWith('Inspect Wall: ') -and $_.Contains('; Wall 2: ') } | Select-Object -Last 1
if ("$line" -notmatch ', (\d+) member\(s\) \(') { Ui-Fail "no member count in: $line" }
$previewed = [int]$Matches[1]
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Wall 2 (wall, lead ' 'Wall 2 placed' 60
Ui-ExpectNew "Frame junction: Wall 2: from Wall 1's side, flush with end A (butt)" 'Wall 2 placed flush with end A'
$placed = [int](Ui-LastMatch '^Placed frame Wall 2 \(wall, lead [^)]*\): (\d+) member')
if ($placed -ne $previewed) { Ui-Fail "Wall 2 placed $placed member(s), the preview showed $previewed" }
if (@(New-Bodies).Count -lt $placed + 1) { Ui-Fail "no backing stud more in Wall 1 after Wall 2" }
Write-Host "ok   Wall 2 flush with Wall 1's end A: $placed members, a backing stud more in Wall 1"
# A click on end B's face, seen from the right (+X: only Wall 1's end B
# shows of it): from that end, a butt corner.
Ui-Step 'the view from the right'        { Ui-ClickLogged 'Orientation cube right' }
Ui-Sync
Start-Sleep -Milliseconds 800
Start-Picks
$ends = @(Wall-Ends $wall1)
if ($ends.Count -lt 1) { Ui-Fail "no end face of Wall 1 to click" }
$end = $ends[0]
Wall-From 'Wall 3' @($end[2], $end[3]) '2000' "from Wall 1's end B (butt corner)" 0
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

# Lapped plates: an L corner near end B, a T mid-side (mitcad#493).
First-Wall 'timber-walls-lapped' 'lapped'
$wall1 = $script:Wall1
Start-Picks
$sides = @(Wall-Sides $wall1)
if ($sides.Count -lt 3) { Ui-Fail "only $($sides.Count) side face(s) of Wall 1 to click" }
$right = $sides | Sort-Object { $_[2] } | Select-Object -Last 1
Wall-From 'Wall 2' @($right[2], $right[3]) '3000' "an L corner at Wall 1's end B (lapped plates)" 1 'lapped'
Start-Picks
$sides = @(Wall-Sides $wall1)
if ($sides.Count -lt 3) { Ui-Fail "only $($sides.Count) side face(s) of Wall 1 to click" }
$xs = @($sides | ForEach-Object { $_[2] } | Sort-Object)
$middle = ($xs[0] + $xs[-1]) / 2
$mid = $sides | Sort-Object { [Math]::Abs($_[2] - $middle) } | Select-Object -First 1
Wall-From 'Wall 3' @($mid[2], $mid[3]) '2000' 'a T on Wall 1, ' 1 'lapped'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
# The next start remembers Lapped plates.
Ui-Mark
Ui-Step 'Wall (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Wall started' 'Wall started again'
Expect-Inspect 'Wall' @('frame wall Wall 4, ', '; corner lapped') 'Lapped plates remembered'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

# The owner's case of mitcad#515: Wall 2 and Wall 3 flush with Wall 1's
# ends, both 3000 mm to -Y (its front side), 3502 mm apart; a click on the
# +X face of Wall 2's middle stud starts Wall 4 on its side towards Wall
# 3: no End clicked, Length Auto, it previews and is placed between the
# two, butting Wall 3's side, 3453 mm from Wall 2's centre line, 3404 mm
# between the faces.
First-Wall 'timber-walls-run' 'butt'
$studs1 = $script:Studs1
Start-Picks
Wall-From 'Wall 2' (Stud-Face $studs1 'first') '3000' "from Wall 1's side, flush with end A (butt)" 1
$studs2 = @(Stud-Bodies)
Start-Picks
Wall-From 'Wall 3' (Stud-Face $studs1 'last') '3000' "from Wall 1's side, flush with end B (butt)" 1
Start-Picks
$side = Stud-Face $studs2 'middle'
Ui-Mark
Ui-Step "the start on Wall 2's side"     { Ui-Click $side[0] $side[1] }
Ui-ExpectNew 'BUILD frame start click: From ' 'the click fills From and Offset'
Ui-ExpectNew "View hint Wall: Click where the wall ends, or press Enter: to Wall 3's side" `
  'the end next, else to the wall the run meets'
Expect-Inspect 'Wall' @("frame wall Wall 4, wall 3453 ${Times} 2400 mm", "; Wall 4: from Wall 2's side, ",
  ", to Wall 3's side (butt), 3404 mm", '; length auto') 'Wall 4 previewed between Wall 2 and Wall 3, no End clicked'
$line = Ui-NewLines |
  Where-Object { $_.StartsWith('Inspect Wall: ') -and $_.Contains("to Wall 3's side") } | Select-Object -Last 1
if ("$line" -notmatch ', (\d+) member\(s\) \(') { Ui-Fail "no member count in: $line" }
$previewed = [int]$Matches[1]
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Wall 4 (wall, lead ' 'Wall 4 placed' 60
Ui-ExpectNew ", to Wall 3's side (butt), 3404 mm" 'Wall 4 placed butting Wall 3'
$placed = [int](Ui-LastMatch '^Placed frame Wall 4 \(wall, lead [^)]*\): (\d+) member')
if ($placed -ne $previewed) { Ui-Fail "Wall 4 placed $placed member(s), the preview showed $previewed" }
# Backing studs in both walls it meets.
if (@(New-Bodies).Count -lt $placed + 2) { Ui-Fail "no backing studs in Wall 2 and Wall 3 for Wall 4" }
Write-Host "ok   Wall 4 between Wall 2 and Wall 3: $placed members, no End clicked"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Write-Host '--- 2. Two points from one stud''s side to the next one''s (mitcad#509)'
Ui-StartApp 'timber-between'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Wall (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Wall started' 'Wall started'
Expect-Inspect 'Wall' @("frame wall Wall 1, wall 3600 ${Times} 2400 mm", 'fasteners yes') 'a wall from the origin along +X'
Ui-Mark
Ui-Step 'no fasteners'                   { Ui-ClickLogged 'Panel Wall input frame_fasteners' }
Expect-Inspect 'Wall' @("frame wall Wall 1, wall 3600 ${Times} 2400 mm", 'fasteners no') 'without fasteners'
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Wall 1 (wall, lead ' 'the wall placed' 60
# Its studs: the bodies of the volume most of its members have.
$volumes = @{}
foreach ($line in @(Ui-NewLines)) {
  if ($line -match '^New body .* \((F[0-9]+\.b[0-9]+)\): volume ([0-9.]+) mm3$') {
    $volumes[$Matches[1]] = [string][Math]::Round([double]$Matches[2])
  }
}
$studVolume = ($volumes.Values | Group-Object | Sort-Object Count -Descending | Select-Object -First 1).Name
$studs = @($volumes.Keys | Where-Object { $volumes[$_] -eq $studVolume })
if ($studs.Count -lt 3) { Ui-Fail "only $($studs.Count) stud(s) of one size in the wall" }
Ui-Step 'Close Wall (Escape)'            { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Timber (F4)'                    { Ui-Key 'F4' }
Ui-ExpectNew 'Command Timber started' 'Timber started'
Ui-Mark
Ui-Step 'Mode: Two points'               { Ui-ClickLogged 'Panel Timber input how=points' }
Ui-ExpectNew 'View hint Timber: Click where the member starts' "the view's hint for the start"
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Step 'the start pick'                 { Ui-ClickLogged 'Panel Timber input start' }
# Each stud's +X side: of its sides in sight, the one furthest right (the
# view from the front right: +X and +Y to the right).
$plusX = @{}
foreach ($s in @(Wall-Sides $studs)) {
  if (-not $plusX.ContainsKey($s[0]) -or $s[2] -gt $plusX[$s[0]][2]) { $plusX[$s[0]] = $s }
}
$studSides = @($plusX.Values | Sort-Object { $_[2] })
if ($studSides.Count -lt 3) { Ui-Fail "only $($studSides.Count) stud side(s) in sight" }
# The second stud's +X side, facing the third, and the third's: not one
# plane. Before mitcad#509 the end was moved onto the start's plane, beside
# the start: a member a few millimetres long, refused.
$first = $studSides[1]
$second = $studSides[2]
Ui-Mark
Ui-Step "the start on a stud's side"     { Ui-Click $first[2] $first[3] }
Ui-ExpectNew 'View hint Timber: Click where it ends; Shift+click places at once' 'the end next'
Ui-Mark
Ui-Step "the end on the next stud's side" { Ui-Click $second[2] $second[3] }
Ui-ExpectNew 'Inspect Timber: points, length ' 'two clicks give the member'
$line = Ui-NewLines |
  Where-Object { $_.StartsWith('Inspect Timber: points, length ') } | Select-Object -Last 1
if ("$line" -notmatch '^Inspect Timber: points, length ([0-9.]+) mm') { Ui-Fail "no length in: $line" }
$length = [double]$Matches[1]
if (-not "$line".Contains('; free')) { Ui-Fail "the member between the studs' sides (two planes) is not free: $line" }
if ($length -lt 450 -or $length -gt 700) { Ui-Fail "the member between two studs 600 apart is $length mm: $line" }
if ("$line" -notmatch '; start on (?!none)' -or "$line" -notmatch ', end on (?!none)') { Ui-Fail "a point not on a stud: $line" }
# The end on the next stud's far side: cut back to its near face.
if ("$line" -notmatch ', end on [^;]* cut;') { Ui-Fail "its end not cut back to the stud's face: $line" }
Write-Host "ok   $length mm between the studs, free, its end on the next stud's face"
Place 'Timber 48x98 test'
Ui-ExpectNew 'by two points, ' 'a member by two points'
$placed = [double](Ui-LastMatch '^Placed Timber 48x98 test as .*, length by two points, ([0-9.]+) mm')
if ([Math]::Abs($placed - $length) -gt 0.1) { Ui-Fail "placed $placed mm long, previewed $length mm" }
Write-Host "ok   placed $placed mm long, as previewed"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Ui-Finish $TimberTest
