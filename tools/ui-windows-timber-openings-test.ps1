# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp tools/cli core/model/src/build core/model-build core/model-api/src/build.rs
# check-all sources: tools/ui-windows-timber-lib.ps1
# Input-driven Windows UI test of Wall's openings and of walls started on
# other bodies' faces (the TIMBER group; mitcad#519, mitcad#524,
# mitcad#536), in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), with the synthetic library of tools/ui-windows-timber-lib.ps1
# and designs made here with mitcad-cli (mitcad#834: once part of
# tools/ui-windows-timber-test.ps1):
#   1. The Openings window (mitcad#519): from Wall's Openings tile, Add
#      window (previewed once, a partition's flat header and no trimmers,
#      marked), Add door, a row typed outside the wall flagged and put
#      back, Load-bearing chosen once (trimmers, headers on edge), placed
#      with both; Timber Frame (MODIFY) on the wall opens the window with
#      them. Clicks, typing and the wheel go to the window itself.
#   2. A Start on another body's face (mitcad#524, the owner's case): a
#      design of a slab and a solid wall with a door (mitcad-cli); a click
#      on the wall's side face starts a wall square from it on the slab's
#      top; on its end face a corner with Butt, in line with Straight (the
#      Corner's icon buttons), placed in line.
#   3. A wall across a long room (mitcad#536, the owner's case): the room's
#      walls one body (mitcad-cli); a start on a long wall's inner face,
#      Length Auto: it previews and is placed as wide as the room, ending at
#      the opposite face of the same body, its plates longer than the
#      timber's longest spliced over studs; the inspection names what the
#      run met and the length, also when a height its studs cannot have
#      refuses the preview.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-openings-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber-openings'),
  [string]$QtBin = '')

$TimberTest = 'UI Windows timber openings test'
. (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
Timber-Init

Write-Host '--- 1. The Openings window: a window and a door, load-bearing or not (mitcad#519)'
# Openings-Place NAME: the place the Openings window logged for NAME, in
# the window ("Openings place offset.2 at x,y; in the window x,y").
function Openings-Place([string]$Name) {
  for ($i = 0; $i -lt 120; $i++) {
    $line = Ui-LastLineWith "Openings place $Name at " -Start
    $match = [regex]::Match("$line", 'in the window (-?\d+),(-?\d+)$')
    if ($match.Success) { return @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value) }
    Start-Sleep -Milliseconds 50
  }
  Ui-Fail "no place of the Openings window logged for '$Name'"
}
function Openings-Window {
  $window = Ui-Windows '^Openings$'
  if ($window.Count -eq 0) { Ui-Fail 'no Openings window' }
  return $window[0]
}
# Openings-Click NAME: a click on that place, posted to the window itself.
function Openings-Click([string]$Name) {
  $place = Openings-Place $Name
  $window = Openings-Window
  $b = $script:UiButtons[1]
  Ui-Apart $window $place[0] $place[1]
  [UiInput]::Move($window, $place[0], $place[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($window, $b[0], $place[0], $place[1], $b[3])
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($window, $b[1], $place[0], $place[1], 0)
}
# Openings-Type NAME TEXT: the field's text replaced and committed (Enter);
# the keys go to the window, then to the main window again.
function Openings-Type([string]$Name, [string]$Text) {
  Openings-Click $Name
  Ui-FocusDialog '^Openings$'
  Ui-Key 'ctrl+a'
  Ui-Type $Text
  Ui-Key 'Return'
  Ui-FocusMain
}
# Openings-Wheel NAME STEPS: a drop-down of the window moved by wheel steps
# (its list cannot open in session 0): -1 chooses the next entry.
function Openings-Wheel([string]$Name, [int]$Steps) {
  $place = Openings-Place $Name
  $window = Openings-Window
  [UiInput]::Move($window, $place[0], $place[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Wheel($window, $place[0], $place[1], $Steps)
}
Ui-StartApp 'timber-openings'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Wall (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Wall started' 'Wall started'
Ui-Mark
Ui-Step 'no fasteners'                   { Ui-ClickLogged 'Panel Wall input frame_fasteners' }
Expect-Inspect 'Wall' @("frame wall Wall 1, wall 3600 ${Times} 2400 mm", 'fasteners no') 'a wall from the origin, no fasteners'
Ui-Mark
Ui-Step 'Openings (its tile)'            { Ui-ClickLogged 'Panel Wall input frame_openings_window' }
Ui-ExpectNew 'Openings window opened for build.wall: 0 row(s), non-load-bearing' 'the Openings window, a partition by default'
Ui-ExpectNew 'Openings place add_window at ' 'its buttons'' places'
Ui-Mark
Ui-Step 'Add window'                     { Openings-Click 'add_window' }
Ui-ExpectNew 'Openings rows: 600 900x1200 900' 'a window written into Wall'
Expect-Inspect 'Wall' @("frame wall Wall 1, wall 3600 ${Times} 2400 mm, 13 member(s)", '1 flat header', '2 king stud',
  '; openings 1, non-load-bearing, opening 1 marked') 'a partition''s window: one flat header, no trimmers, marked'
Ui-Sync
Start-Sleep -Milliseconds 1500
$previews = @(Ui-NewLines |
  Where-Object { $_.StartsWith('Inspect Wall: ') -and $_.Contains('; openings 1, non-load-bearing') })
if ($previews.Count -ne 1) { Ui-Fail "the window's change previewed $($previews.Count) times" }
Write-Host 'ok   one preview for the change'
Ui-Mark
Ui-Step 'Add door'                       { Openings-Click 'add_door' }
Ui-ExpectNew 'Openings rows: 600 900x1200 900; 2100 900x2100' 'a door after the window'
Expect-Inspect 'Wall' @('16 member(s)', '2 flat header', '4 king stud', '; openings 2, non-load-bearing') `
  'the door framed too'
Ui-Mark
Ui-Step 'row 2 at 4000'                  { Openings-Type 'offset.2' '4000' }
Ui-ExpectNew 'Openings row 2: outside the wall (3600 mm long)' 'the row outside the wall flagged'
Ui-Mark
Ui-Step 'row 2 at 2100 again'            { Openings-Type 'offset.2' '2100' }
Ui-ExpectNew 'Openings rows ok' 'the row fits again'
Ui-Mark
Ui-Step 'Load-bearing'                   { Openings-Wheel 'bearing' -1 }
Ui-ExpectNew 'Openings bearing: load-bearing' 'the wall made load-bearing'
Expect-Inspect 'Wall' @('22 member(s)', '4 header', '4 trimmer stud', '; openings 2, load-bearing') `
  'load-bearing: trimmer studs and headers on edge'
# The Openings window takes the keyboard when it opens (its Enter commits
# a field): closed (Close; the openings stay), it gives the main window
# the activation back (mitcad#532), and Enter places the wall.
Ui-Mark
Ui-Step 'close the window (Close)'       { Openings-Click 'close' }
# (Logged "Openings window hidden: the main window active again" when it
# still had the activation as it hid.)
Ui-FocusMain
Ui-Sync
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Wall 1 (wall, lead ' 'the wall placed with both' 60
$placed = [int](Ui-LastMatch '^Placed frame Wall 1 \(wall, lead [^)]*\): (\d+) member')
if ($placed -ne 22) { Ui-Fail "Wall 1 placed with $placed member(s), not 22" }
# Its plates (the largest bodies): one a click picks is chosen below.
$plates = @(Largest-Bodies)
# (The window closes with Wall when it is open: the Linux test checks it.)
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Timber Frame (F12)'             { Ui-Key 'F12' }
Ui-ExpectNew 'Command Timber Frame started' 'Timber Frame started'
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Step 'its Member'                     { Ui-ClickLogged 'Panel Timber Frame input part' }
Ui-Sync
Ui-Mark
$plate = Pickable $plates
if (-not $plate) { Ui-Fail "no plate of the wall ($($plates -join ', ')) to pick" }
Ui-Step 'a member of the wall'           { Ui-ClickPick "$plate/" }
Expect-Inspect 'Timber Frame' @('frame Wall 1 laid out again: 22 member(s)', '; openings 2, load-bearing') `
  'the wall''s values, load-bearing as placed'
Ui-Mark
Ui-Step 'Openings (its tile)'            { Ui-ClickLogged 'Panel Timber Frame input frame_openings_window' }
Ui-ExpectNew 'Openings window opened for build.frame: 2 row(s), load-bearing' 'the window with the wall''s window and door'
# Closed with its Close button first: it has the keyboard (above).
Ui-Step 'close the window (Close)'       { Openings-Click 'close' }
Ui-FocusMain
Ui-Step 'Cancel (Escape)'                { Ui-Key 'Escape' }

Write-Host '--- 2. A Start on another body''s face (mitcad#524): a solid wall with a door on a slab'
# A slab 5000 x 4000 x 200 (F1), a solid wall 3000 x 200 x 2700 on it (F2)
# and a door 900 x 2100 cut through the wall only (F3).
$solidJson = Join-Path $work 'solid-wall.json'
$solidFile = Join-Path $work 'solid-wall.mitcad'
Set-Content -Encoding ascii $solidJson @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-1000, -2000],
    "length": 5000, "width": 4000, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 3000, "width": 200, "height": 2700, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [1000, -10],
    "length": 900, "width": 220, "height": 2100, "operation": "cut", "participants": ["F2.b0"]}}
]
'@
Run-Cli 'run solid-wall' @('run', $solidJson, '--save', $solidFile)
Ui-StartApp 'timber-solid-wall' @('--open', $solidFile)
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Wall (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Wall started' 'Wall started'
# Solid-Face TEXT DESCRIPTION: a click on each face of the solid wall
# (F2.b0) a click on Start picks, Start picked again before each, until the
# preview's inspection has TEXT. Each face is clicked where the newest
# "Pick places:" puts it: F6 fits the preview too, so the view moves with
# the last click's preview (a wall from a side face reaches past the slab),
# and a place logged in another view picks another face (mitcad#1058).
function Solid-Places {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  $places = [ordered]@{}
  foreach ($line in $lines[$start..($lines.Count - 1)]) {
    if ($line -match '^Pick face (F2\.b0/\S*) at (\d+),(\d+)$') {
      $places[$Matches[1]] = @([int]$Matches[2], [int]$Matches[3])
    }
  }
  return $places
}
function Solid-Face([string]$Text, [string]$Description) {
  Start-Picks
  Ui-Sync
  $faces = @((Solid-Places).Keys)
  if ($faces.Count -lt 1) { Ui-Fail 'no face of the solid wall to click' }
  $first = $true
  foreach ($face in $faces) {
    if (-not $first) { Start-Picks; Ui-Sync }
    $first = $false
    $at = (Solid-Places)[$face]
    if (-not $at) { Write-Host "     $face not in sight in this view"; continue }
    Ui-Mark
    Ui-Step "a face of the solid wall ($face)" { Ui-Click $at[0] $at[1] }
    # The click's preview: once it is inspected, what the app computes for
    # it is waited for and the last inspection decides; a face that gives
    # none in 1.5 s gives nothing (once 3 s for every face that did not
    # give TEXT, mitcad#834).
    $settled = $false
    for ($k = 0; $k -lt 60; $k++) {
      $line = Ui-NewLines | Where-Object { $_.StartsWith('Inspect Wall: ') } |
        Select-Object -Last 1
      if ("$line".Contains($Text)) { Write-Host "ok   ${Description}: $line"; return }
      if ($line -and -not $settled) {
        Ui-WaitIdle
        Start-Sleep -Milliseconds 300
        Ui-Sync
        $settled = $true
        continue
      }
      if ($settled -or $k -ge 30) { break }
      Start-Sleep -Milliseconds 50
    }
  }
  Ui-Fail "${Description}: no face of the solid wall gave '$Text'"
}
# A side face: square from it on its outward (visible) side, on the slab.
Solid-Face 'from a face of ' 'a wall from a face of the solid wall'
Expect-Inspect 'Wall' @(', base on ', '(z 200 mm)', '; corner butt') 'on the slab''s top, the bottom plate at z 200'
# Its end face: a corner with Butt, in line with Straight (the Corner's
# icon buttons).
Solid-Face 'from the end face of ' 'a corner on the solid wall''s end face'
Ui-Mark
Ui-Step 'Corner: Straight'               { Ui-ClickLogged 'Panel Wall input frame_corner=straight' }
Expect-Inspect 'Wall' @('in line beyond the end face of ', '(straight)', '; corner straight') 'Straight: in line beyond it'
Ui-Mark
Ui-Step 'Corner: Butt'                   { Ui-ClickLogged 'Panel Wall input frame_corner=butt' }
Expect-Inspect 'Wall' @('from the end face of ', '(butt corner)') 'Butt again: the corner'
Ui-Mark
Ui-Step 'Corner: Straight again'         { Ui-ClickLogged 'Panel Wall input frame_corner=straight' }
Expect-Inspect 'Wall' @('in line beyond the end face of ') 'Straight again'
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Wall 1 (wall, lead ' 'the wall placed in line with the solid wall' 60
Ui-ExpectNew 'Frame junction: Wall 1: in line beyond the end face of ' 'placed in line beyond its end face'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Write-Host '--- 3. The owner''s case of mitcad#536: across a long room, Length Auto, its plates spliced'
# A slab (F1) and a room's walls as one body (F2): a box 10400 x 8219 x
# 2700 with the room, 10000 x 7819 inside, cut out of it (F3). A start on
# a long wall's inner face, Length Auto: the run meets the opposite wall,
# a face of the same body, 7819 mm on; 7819 mm is longer than the timber's
# longest (6000 mm), so each plate is three pieces of at most 3600 mm (its
# longest stock length), the joints over the studs at 3600 and 7200.
$roomJson = Join-Path $work 'long-room.json'
$roomFile = Join-Path $work 'long-room.mitcad'
Set-Content -Encoding ascii $roomJson @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-700, -700],
    "length": 11400, "width": 9219, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-200, -200],
    "length": 10400, "width": 8219, "height": 2700, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10000, "width": 7819, "height": 2800, "operation": "cut", "participants": ["F2.b0"]}}
]
'@
Run-Cli 'run long-room' @('run', $roomJson, '--save', $roomFile)
Ui-StartApp 'timber-long-room' @('--open', $roomFile)
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Wall (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Wall started' 'Wall started'
# The faces of the room's body clicked in turn until one runs across the
# room (a long wall's inner face; the others run 10000 mm or meet nothing).
Solid-Face "frame wall Wall 1, wall 7819 ${Times} 2400 mm" 'a wall from a long wall''s inner face, across the room'
Expect-Inspect 'Wall' @("frame wall Wall 1, wall 7819 ${Times} 2400 mm", '3 bottom plate', '3 top plate', '; length auto',
  '; Length Auto: the run met a face of ', ', the wall 7819 mm',
  '; spliced top plate 3600 7200, bottom plate 3600 7200') `
  'previewed across the room, the run''s end named, each plate three pieces over the studs at 3600 and 7200'
Expect-Inspect 'Wall' @(', to a face of ', ', 7819 mm', ', base on ') 'the junction: to the opposite wall''s face, on the slab'
# Refused (its studs 6904 mm, longer than the timber's longest): the panel
# still names the wall, its length and what the run met.
Ui-Mark
Ui-Step '7000 mm high'                   { Ui-TypeIn 'Panel Wall input frame_height' '7000' }
Ui-ExpectNew "Frame preview refused: frame wall Wall 1, wall 7819 ${Times} 7000 mm, 0 member(s)" `
  'the refused preview names the wall and the length worked out'
$line = Ui-NewLines | Where-Object { $_.StartsWith('Frame preview refused: ') } |
  Select-Object -Last 1
if (-not "$line".Contains('; Length Auto: the run met a face of ')) { Ui-Fail "the refused preview without the run's end: $line" }
if (-not "$line".Contains("longer than the part's longest")) { Ui-Fail "the refused preview without its reason: $line" }
Write-Host 'ok   refused, with what the run met and why'
Ui-Mark
Ui-Step '2400 mm high again'             { Ui-TypeIn 'Panel Wall input frame_height' '2400' }
Expect-Inspect 'Wall' @("frame wall Wall 1, wall 7819 ${Times} 2400 mm", '; spliced top plate 3600 7200, bottom plate 3600 7200') `
  '2400 mm high again: previewed'
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed frame Wall 1 (wall, lead ' 'the wall placed across the room' 60
Ui-ExpectNew 'Frame spliced: top plate 3600 7200, bottom plate 3600 7200' 'placed with its plates spliced'
$line = Ui-NewLines |
  Where-Object { $_.StartsWith('Frame junction: Wall 1: from a face of ') } | Select-Object -Last 1
if (-not ("$line".Contains(', to a face of ') -and "$line".Contains(', 7819 mm'))) {
  Ui-Fail "Wall 1 placed without the face its run met: $line"
}
Write-Host "ok   $line"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Ui-Finish $TimberTest
