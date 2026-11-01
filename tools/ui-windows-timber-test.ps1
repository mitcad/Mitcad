# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp tools/cli core/model/src/build core/model-build core/model-api/src/build.rs
# check-all sources: tools/ui-windows-timber-lib.ps1
# Input-driven Windows UI test of the BUILD tab's TIMBER group's Timber
# tool (mitcad#478, mitcad#485), in the build VM (tools/ui-windows-lib.ps1
# says how the input is posted), with the synthetic library of
# tools/ui-windows-timber-lib.ps1; frames, walls, openings, fasteners,
# floors and roofs are the other tools/ui-windows-timber-*-test.ps1, one
# ctest each (mitcad#834):
#   1. Profile's picker lists no timber, Timber's no profile, its standard
#      size first; Timber's first start (mitcad#485): On a member, a typed
#      length of the part's default (3000 mm), along X, flat, the
#      dimensioned 48x98, resting on the XY plane (mitcad#492).
#   2. On a member: a post standing on the XY plane (Enter at the origin;
#      after Place nothing previewed at the origin on top of it, the panel
#      waiting for a click on a member, mitcad#513),
#      then a member on the post's side: the hover names only the face a
#      click picks, nothing previewed before the click; once picked, the
#      mouse along the side, off the parts and back previews and inspects
#      nothing, the preview's end A unchanged, a click on another face
#      moves it once and a click at the first place back (mitcad#508); the
#      next start takes the length typed last and the direction
#      (mitcad#485).
#   3. Two points (mitcad#485): a member from a click on the second
#      member's side to a click on the post's side, nothing previewed
#      towards the mouse between the clicks; Tab turns it on edge.
#   4. Spacing: members at a centre distance along the second member's
#      side; the hover names only the face a click picks, nothing previewed
#      or inspected before the click (mitcad#489); every member previewed
#      after the click, which stays there while the mouse moves, a click on
#      another face moving it once (mitcad#508), and again when the
#      distance changes their count (mitcad#487) and again when the Offset
#      typed after the click moves the set from end A, chosen in From
#      (mitcad#492), placed as one set
#      of as many; Spacing Set edits its distance.
#   5. Connector at a timber junction: wood screws by default, sized from
#      the members, one click.
#   6. Profile again: the design's timber is not in its picker; Cut List:
#      the timber rows.
#   7. Every size a tile of its own (mitcad#484): the picker lists the 48
#      member's three rows and the 28 one, the families as groups, a row's
#      own picture; starring 48x148 stars that size only (mitcad#491): it
#      comes first and Favorites lists it alone; a search by 48x148 finds
#      its tile, which places that row with its section. The star changes
#      only the star: the tiles stay where they are until the list is made
#      again (mitcad#517). The use count is per size (mitcad#518): only the
#      48x98 tile counts the design's members; once a 48x148 member is
#      placed, that tile counts it, and In this design lists the two sizes.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber'),
  [string]$QtBin = '')

$TimberTest = 'UI Windows timber test'
. (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
Timber-Init

# A new design (mitcad#394): Component1 active.
Ui-StartApp 'timber-new'
Ui-Mark
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-ExpectNew 'Ribbon BUILD/PROFILE at' 'its PROFILE group'
Ui-ExpectLog 'Ribbon button build.timber at' 'the Timber button on the BUILD tab'

Write-Host '--- 1. The pickers, the first start'
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'BUILD parts profile: 1 (0 in the design)' "Profile's picker: the slot profile only, no timber"
Ui-ExpectNew 'Panel Profile tile part: Profile 20 test' 'Profile starts with the slot profile'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'the Timber button'              { Ui-ClickLogged 'Ribbon button build.timber' }
Ui-ExpectNew 'Command Timber started' 'Timber started'
Ui-ExpectNew 'BUILD parts timber: 4 (0 in the design)' "Timber's picker: the members' four sizes (mitcad#484), no profile"
Ui-ExpectNew "BUILD timber starts with test-timber/timber-48-test: the first time's dimensioned 48x98" 'the dimensioned 48x98 the first time'
Ui-ExpectNew 'BUILD timber adds to Component1, member' 'On a member at first (mitcad#485)'
Ui-ExpectNew "BUILD timber starts: length 3000 mm (the part's default), typed; along x, flat" "a typed length, the part's default, along X, flat"
Expect-Inspect 'Timber' @('place grounded, length 3000.00 mm (typed)', '; along x, flat', '; thickness along Z',
                          '; end A at 0.0, 0.0, 24.0', '; part Timber 48x98 test') `
  'the preview: 3000 mm along X, lying flat, resting on the XY plane (mitcad#492)'

Write-Host '--- 2. On a member, the next start'
Ui-Step '600 mm long'                    { Ui-TypeIn 'Panel Timber input length' '600' }
Ui-Mark
Ui-Step 'standing (Z)'                   { Ui-ClickLogged 'Panel Timber input axis=z' }
Expect-Inspect 'Timber' @('place grounded, length 600.00 mm (typed)', '; along z') 'a post at the origin, standing'
Place 'Timber 48x98 test'
$post = Placed-Body
if (-not $post) { Ui-Fail 'no body of the post' }
# After Place no ghost (mitcad#513): the next member waits for a click on a
# member, nothing previewed at the origin on top of the post.
Expect-Inspect 'Timber' @('failed: Click a member''s side or end.') 'after Place the panel waits for a click on a member'
No-Ghost 'Timber' 'place grounded' 'no member previewed at the origin on top of the post'
Refresh-Picks 'at'
$postSides = @(Faces-Of $post $side)
if ($postSides.Count -eq 0) { Ui-Fail "no side of the post ($post) to pick" }
# Nothing follows the mouse (mitcad#508): the hover names the face a click
# picks; once the side is picked, the mouse moving runs no placement and
# no inspection, and a click on another face moves the preview once.
$postAt = Pick-At $post $postSides[0]
Hover-Names 'Timber' $post $postSides[0] $postAt
Ui-Mark
Ui-Step "the post's side"                { Ui-Click $postAt[0] $postAt[1] }
Ui-ExpectNew 'Inspect Timber: place side' "on the post's side, a free offset"
$postOther = @($postSides | Select-Object -Skip 1) + @(Faces-Of $post ':end_b') | Select-Object -First 1
if (-not $postOther) { Ui-Fail "no other face of the post ($post) to click" }
Keeps-Place 'Timber' $postAt $post $postOther
Place 'Timber 48x98 test'
$beam = Placed-Body
if (-not $beam) { Ui-Fail 'no body of the member on the post' }
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Timber (F4)'                    { Ui-Key 'F4' }
Ui-ExpectNew 'Command Timber started' 'Timber again'
Ui-ExpectNew 'BUILD timber starts with test-timber/timber-48-test: chosen last in the tool' 'the part chosen last'
Ui-ExpectNew 'BUILD timber adds to Component1, member' 'the mode used last'
Ui-ExpectNew 'BUILD timber starts: length 600 mm (typed last), typed; along z, flat' 'the length typed last and the direction'

Write-Host '--- 3. Two points'
Ui-Mark
Ui-Step 'Two points'                     { Ui-ClickLogged 'Panel Timber input how=points' }
Ui-ExpectNew 'View hint Timber: Click where the member starts' "the view's hint for the start"
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Step 'the start pick'                 { Ui-ClickLogged 'Panel Timber input start' }
Ui-Mark
$beamSides = @(Faces-Of $beam $side)
if ($beamSides.Count -eq 0) { Ui-Fail "no side of the member ($beam) to start on" }
Ui-Step "the start on the member's side" { Pick-Face $beam $beamSides[0] }
Ui-ExpectNew 'View hint Timber: Click where it ends; Shift+click places at once' 'the end next'
$postSides = @(Faces-Of $post $side)
if ($postSides.Count -eq 0) { Ui-Fail "no side of the post ($post) to end at" }
$at = Pick-At $post $postSides[0]
Ui-Mark
# MK_SHIFT keeps the move in session 0 (tools/ui-windows-build-test.ps1).
Ui-Step "the mouse over the post's side" { [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0x04) }
Start-Sleep -Milliseconds 1500
Ui-Sync
$towards = @(Ui-NewLines | Where-Object { $_.StartsWith('Inspect Timber: points') })
if ($towards.Count -gt 0) { Ui-Fail "a member previewed towards the mouse before the second click: $($towards[0])" }
Write-Host 'ok   nothing follows the mouse before the second click'
Ui-Mark
Ui-Step 'the end there'                  { Ui-Click $at[0] $at[1] }
Ui-ExpectNew 'Inspect Timber: points, length ' 'two clicks give the member'
Ui-ExpectNew '; length by two points' 'its length from the points'
Ui-Mark
Ui-Step 'Tab: on edge'                   { Ui-Key 'Tab' }
Ui-ExpectNew 'Inspect Timber: points, length ' 'previewed again'
Ui-ExpectNew ' mm, edge; ' 'on edge'
Place 'Timber 48x98 test'
Ui-ExpectNew 'by two points, ' 'a member by two points'

Write-Host '--- 4. Spacing'
Ui-Mark
Ui-Step 'Spacing'                        { Ui-ClickLogged 'Panel Timber input how=spacing' }
Ui-ExpectNew 'Panel Timber input distance at' 'its distance'
Ui-Step '200 mm long'                    { Ui-TypeIn 'Panel Timber input length' '200' }
Refresh-Picks 'at'
$beamSides = @(Faces-Of $beam $side)
if ($beamSides.Count -eq 0) { Ui-Fail "no side of the member ($beam) for the set" }
# The hover shows only the face a click picks (mitcad#489): no set laid out
# while the mouse moves, no inspection asked.
$at = Pick-At $beam $beamSides[0]
Hover-Names 'Timber' $beam $beamSides[0] $at
Ui-Mark
Ui-Step "the member's side"              { Ui-Click $at[0] $at[1] }
Ui-ExpectNew 'Inspect Timber: place side' 'on its side'
Ui-ExpectNew '; set of ' 'the set counted'
Ui-ExpectNew 'center distance 400 mm' 'at 400 mm'
$at400 = Set-Previewed 'center distance 400 mm' 'at 400 mm'
# Once picked the set stays there while the mouse moves (mitcad#508); a
# click on another face moves it once, a click at the first place back.
$setOther = $null
$setOtherBody = $beam
if ($beamSides.Count -gt 1) { $setOther = $beamSides[1] }
else {
  $setOtherBody = $post
  $setOther = @(Faces-Of $post $side) | Select-Object -First 1
}
if (-not $setOther) { Ui-Fail "no other face of the member ($beam) or the post ($post) to click" }
Keeps-Place 'Timber' $at $setOtherBody $setOther
$back400 = Set-Previewed 'center distance 400 mm' 'at the first place again'
if ($back400 -ne $at400) { Ui-Fail "the set at the first place again previewed $back400 members, not the $at400 before" }
Ui-Mark
Ui-Step 'a distance of 200'              { Ui-TypeIn 'Panel Timber input distance' '200' }
$at200 = Set-Previewed 'center distance 200' 'at 200'
if ($at200 -le $at400) { Ui-Fail "200 mm previewed $at200 members, not more than the $at400 at 400 mm" }
# The Offset typed after the click moves the set (mitcad#492): from its
# reference it starts 300 mm in, fewer members; back to 0, as before.
# The reference is chosen first: the click's depends on where on the side
# the pick place lies (the middle here: 300 mm from it ran off the beam,
# clamped to end B, mitcad#740). From End A, the list's first entry: a
# drop-down's list cannot open in session 0, so wheel steps up the list,
# one message a step (tools/ui-windows-mount-test.ps1); at the top a step
# changes nothing.
Ui-Step 'From End A'                     {
  Ui-WheelLogged 'Panel Timber input from' 1
  Start-Sleep -Milliseconds 150
  Ui-WheelLogged 'Panel Timber input from' 1
}
Ui-Sync
Ui-Mark
Ui-Step 'an offset of 300'               { Ui-TypeIn 'Panel Timber input offset' '300' }
Expect-Inspect 'Timber' @(' at 300.0 mm from a', 'center distance 200') 'the set previewed 300 mm from end A'
$at300 = Set-Previewed ' at 300.0 mm from a' 'offset 300'
if ($at300 -ge $at200) { Ui-Fail "offset 300 previewed $at300 members, not fewer than the $at200 at offset 0" }
Ui-Mark
Ui-Step 'the offset back to 0'           { Ui-TypeIn 'Panel Timber input offset' '0' }
$again0 = Set-Previewed ' at 0.0 mm from a' 'offset 0 again'
if ($again0 -ne $at200) { Ui-Fail "offset 0 previewed $again0 members, not the $at200 before" }
Place 'Timber 48x98 test'
Ui-ExpectNew '; a set of ' 'placed as one set'
$set = Ui-LastMatch '; a set of (\d+)'
if ([int]"0$set" -ne $at200) { Ui-Fail "the set placed $set members, the preview showed $at200" }
Write-Host "ok   placed the $set members previewed"
$setBody = Placed-Body
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Spacing Set (F11)'              { Ui-Key 'F11' }
Ui-ExpectNew 'Command Spacing Set started' 'Spacing Set started'
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Step "Spacing Set's member"           { Ui-ClickLogged 'Panel Spacing Set input part' }
$setFaces = @(Faces-Of $setBody $side)
if ($setFaces.Count -eq 0) { Ui-Fail "no face of a member of the set ($setBody)" }
Ui-Mark
Ui-Step 'a member of the set'            { Pick-Face $setBody $setFaces[0] }
Ui-ExpectNew 'Inspect Spacing Set: set ' 'the set'
Ui-Step 'a distance of 150'              { Ui-TypeIn 'Panel Spacing Set input distance' '150' }
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Spacing set of ' 'laid out again'
$again = Ui-LastMatch '^Spacing set of .*: (\d+) member'
if ([int]"0$again" -le [int]"0$set") { Ui-Fail "150 mm gave $again members, not more than the $set at 400 mm" }
Write-Host "ok   $set members at 400 mm, $again at 150 mm"

Write-Host '--- 5. Connector at a timber junction'
Ui-Mark
Ui-Step 'Connector (F8)'                 { Ui-Key 'F8' }
Ui-ExpectNew 'Inspect Connector: junction ' 'a junction chosen'
Ui-ExpectNew 'preview Wood screw head test with 2 fastener(s)' 'two sized countersunk wood screws by default (mitcad#516)'
Ui-ExpectNew ', size ' 'sized from the members'
Ui-Mark
Ui-Step 'Connect (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Connected Wood screw head test at junction ' 'one click finishes the joint' 30
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Write-Host '--- 6. Profile again, the cut list'
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'BUILD parts profile: 1 (0 in the design)' "the design's timber is not in Profile's picker"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Cut List (F9)'                  { Ui-Key 'F9' }
Ui-FocusDialog '^Cut List$'
Ui-ExpectNew 'Cut list row: ' "the cut list's rows"
Ui-ExpectNew 'x Timber 48x98 test' 'the members in it'
Ui-ExpectNew 'Cut list timber: ' 'the timber columns and totals'
Ui-ExpectNew 'Cut list timber row: Timber 48x98 test' "a member's material and sizes"
Ui-ExpectNew 'columns Quantity, Component, Length (mm)' 'the timber columns after the others'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 7. Every size a tile of its own, the families as groups'
Ui-Mark
Ui-Step 'Timber (F4)'                    { Ui-Key 'F4' }
Ui-ExpectNew 'Command Timber started' 'Timber again'
Ui-Mark
Ui-Step 'the part picker (the Component tile)' { Ui-ClickLogged 'Panel Timber input part' }
Ui-ExpectNew 'Part picker opened: 4 parts for build.timber' 'the picker lists every size'
Ui-ExpectNew 'Part picker group All components: 4 parts, first test-timber/timber-48-test (48x98)' 'all four sizes, the standard ones first'
# The use count is per size (mitcad#518): the design's members are all
# 48x98, so only that tile has a count, not every size of the family.
$tiles = $null
for ($i = 0; $i -lt 120 -and "$tiles" -notmatch 'timber-48-test \(48x98\) x\d+'; $i++) {
  $tiles = Ui-LastMatch '^Part picker tiles: (.*)$'
  if ("$tiles" -notmatch 'timber-48-test \(48x98\) x\d+') { Start-Sleep -Milliseconds 50 }
}
if ("$tiles" -notmatch 'timber-48-test \(48x98\) x\d+') { Ui-Fail "the 48x98 tile has no count: '$tiles'" }
if ("$tiles" -match '\(48x148\) x|\(48x198\) x|timber-28-test x') {
  Ui-Fail "a size the design does not use has a count: '$tiles'"
}
Write-Host "ok   only the size the design uses has a count: $tiles"
foreach ($tile in @('test-timber/timber-48-test (48x98)', 'test-timber/timber-48-test (48x148)',
                    'test-timber/timber-48-test (48x198)', 'test-timber/timber-28-test')) {
  Ui-ExpectNew "Part picker tile $tile at " "a tile of $tile"
}
Ui-ExpectNew 'Part picker group row Dimensioned timber at ' 'the dimensioned timber a group of its own'
Ui-ExpectNew 'Part picker group row Planed timber at ' 'the planed timber a group of its own'
Ui-ExpectLog 'BUILD preview image of test-timber/timber-48-test: 16x16' "the part's picture in the panel's tile"
# A size's star stars that size only (mitcad#491), not its family's rows.
Ui-Mark
Ui-Step 'the 48x148 tile'                { Picker-Click 'Part picker tile test-timber/timber-48-test (48x148)' }
Ui-ExpectNew 'Part picker chose test-timber/timber-48-test (48x148)' '48x148 chosen'
# The panel reads a picture once a session: the row's own, now.
Ui-ExpectNew 'BUILD preview image of test-timber/timber-48-test (48x148): 24x24' "the row's own picture in the tile"
$tilesBefore = Ui-LastMatch '^Part picker tiles: (.*)$'
Ui-Mark
Ui-Step 'star it (Star)'                 { Picker-Click 'Part picker place star' }
Ui-ExpectNew 'Part picker star timber-48-test (48x148) on' 'the size 48x148 starred'
# A star changes only the star (mitcad#517): the tiles stay where they are
# and the selection on the starred one; the size comes first once the list
# is made again (Favorites, then All components below).
Ui-ExpectNew "Part picker tiles kept: $tilesBefore; current test-timber/timber-48-test (48x148)" 'the tiles stay in their places, the selection on the size'
Ui-Mark
Ui-Step 'Favorites'                     { Picker-Click 'Part picker group row Favorites' }
Ui-ExpectNew 'Part picker group Favorites: 1 parts, first test-timber/timber-48-test (48x148)' 'Favorites lists that size alone'
Ui-Mark
Ui-Step 'All components'                      { Picker-Click 'Part picker group row All components' }
Ui-ExpectNew 'Part picker group All components: 4 parts, first test-timber/timber-48-test (48x148)' 'every size again'
Ui-ExpectNew 'Part picker tile test-timber/timber-48-test (48x98) at ' "48x98's tile after the starred one"
Ui-Mark
Ui-Step 'the 48x98 tile again'           { Picker-Click 'Part picker tile test-timber/timber-48-test (48x98)' }
Ui-ExpectNew 'Part picker chose test-timber/timber-48-test (48x98)' '48x98 chosen again'
Ui-Step 'the search field'               { Picker-Click 'Part picker place search' }
Ui-Mark
Ui-Step 'search 48x148'                  { Picker-Type '48x148' }
Ui-ExpectNew "Part picker search '48x148'" 'searched by the size'
Ui-ExpectNew ': 1 parts, first test-timber/timber-48-test (48x148)' 'the search finds the size'
Ui-ExpectNew 'Part picker tile test-timber/timber-48-test (48x148) at ' 'its tile'
Ui-Mark
Ui-Step 'the 48x148 tile'                { Picker-Click 'Part picker tile test-timber/timber-48-test (48x148)' }
Ui-ExpectNew 'Part picker chose test-timber/timber-48-test (48x148)' '48x148 chosen'
Ui-ExpectNew 'Panel Timber tile part: Timber 48x98 test (48x148)' "the panel's tile names the size"
Ui-ExpectLog 'BUILD preview image of test-timber/timber-48-test (48x148): 24x24' "the row's own picture (read when first chosen)"
Ui-Step 'On a member'                    { Ui-ClickLogged 'Panel Timber input how=member' }
Refresh-Picks 'at'
# A side of any member a click picks now.
$lines = @(Ui-LogLines)
$from = 0
for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $from = $i; break } }
$onSide = $lines[$from..($lines.Count - 1)] | ForEach-Object {
  if ($_ -match '^\s*Pick face (\S+)/(\S*:part\(F2:side\(\S*) at \d+,\d+$') { , @($Matches[1], $Matches[2]) } } |
  Select-Object -First 1
if (-not $onSide) { Ui-Fail 'no side of a member to place on' }
Ui-Mark
Ui-Step "a member's side"                { Pick-Face $onSide[0] $onSide[1] }
Ui-ExpectNew 'Inspect Timber: place side' "on a member's side"
Place 'Timber 48x98 test'
$length = [double](Ui-LastMatch '^Placed Timber 48x98 test as .* L=([0-9.]+)')
$volume = [double](Ui-LastMatch '^New body .* \(F[0-9]+\.b[0-9]+\): volume ([0-9.]+) mm3')
$wanted = 148 * 48 * $length
if ([math]::Abs($volume - $wanted) -gt 1) { Ui-Fail "the 48x148 member is $volume mm3, not 148 x 48 x $length = $wanted" }
Write-Host "ok   placed as 48x148: $volume mm3 (148 x 48 x $length)"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
# Two sizes of one family in the design: each tile counts its own uses
# (mitcad#518), the sizes not used have none.
Ui-Mark
Ui-Step 'Timber (F4)'                    { Ui-Key 'F4' }
Ui-ExpectNew 'Command Timber started' 'Timber again'
Ui-Mark
Ui-Step 'the part picker (the Component tile)' { Ui-ClickLogged 'Panel Timber input part' }
Ui-ExpectNew 'Part picker opened: 4 parts for build.timber' 'the picker again'
# The search 48x148 above stays in the picker (it shows the part chosen):
# cleared, every size is a tile again.
Ui-Step 'the search field'               { Picker-Click 'Part picker place search' }
Ui-Mark
Ui-Step 'clear the search'               { Picker-Keys @(0x23) 1; Picker-Keys @(0x08) 6 }
Ui-ExpectNew "Part picker search ''" 'the search cleared'
Ui-ExpectNew 'Part picker group All components: 4 parts' 'every size again'
$tiles = $null
for ($i = 0; $i -lt 120 -and "$tiles" -notmatch 'timber-48-test \(48x148\) x1'; $i++) {
  $tiles = Ui-LastMatch '^Part picker tiles: (.*)$'
  if ("$tiles" -notmatch 'timber-48-test \(48x148\) x1') { Start-Sleep -Milliseconds 50 }
}
if ("$tiles" -notmatch 'timber-48-test \(48x148\) x1') { Ui-Fail "the 48x148 tile does not count its one member: '$tiles'" }
if ("$tiles" -notmatch 'timber-48-test \(48x98\) x\d+') { Ui-Fail "the 48x98 tile lost its count: '$tiles'" }
if ("$tiles" -match '\(48x198\) x|timber-28-test x') { Ui-Fail "a size the design does not use has a count: '$tiles'" }
Write-Host "ok   per-size counts: $tiles"
Ui-Mark
Ui-Step 'In this design'                 { Picker-Click 'Part picker group row In this design' }
Ui-ExpectNew 'Part picker group In this design: 2 parts, first test-timber/timber-48-test (48x148)' 'In this design lists only the two sizes the design uses'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Ui-Finish $TimberTest
