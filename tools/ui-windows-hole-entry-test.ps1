# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/commands/HoleCommands.cpp app/commands/PlaceInputs.cpp
# check-all sources: core/model/src/features/hole.rs tools/cli
# Input-driven Windows UI test of Hole's keyboard and its dimensions in the
# view (mitcad#1181) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), on the demo block (60 x 40 x 20, Body1, F2.b0); the
# Linux counterpart is tools/ui-hole-entry-test.sh:
#   1. In the Floating and the Docked layout: Hole on the top face,
#      Distance 1 and Distance 2 typed (15, 20). Tab in Distance 2 moves
#      the keyboard on to Diameter, which takes what is typed next (10);
#      Shift+Tab goes back to Distance 2, and again to Distance 1.
#   2. Floating: the dimensions in the view take typed values. Distance
#      2's value clicked, 25 typed and Esc: the box closes and 20 is back.
#      Depth's value clicked, 12 and Enter: Depth is 12 and the preview
#      follows. Depth's again, "abc": the box turns to the error colour,
#      Enter is refused (the box stays, the command too), Esc puts 12 back.
#      Depth's again, 10 and Enter. OK makes the hole 15 and 20 from the
#      edges, 10 across and 10 deep.
#   3. Floating (mitcad#1285): Hole at the top face's center, 10 across;
#      the edge across the face picked through the Reference 2 row: no
#      failure notice, the hole where it was, Distance 1 and 2 evaluated,
#      Distance 2 the distance from the new edge. Then Distance 1 typed 12
#      and the edge across from Reference 1 picked through its row (or,
#      when the view hides that edge, the same with Reference 2's first
#      edge): the hole stays, the distance is the rest of the face's width.
#      OK makes the hole.
#   4. Print Shape with and without a print bed face (mitcad#1744), on a
#      60 x 40 x 20 block that mitcad-cli makes with a 6 mm hole 10 deep
#      lying in it along +Y (Hole1 F3): without a print bed face Hole1
#      edited shows Print Shape, Teardrop chosen and saved, its roof to +Z
#      (the saved design's roof faces); with the block's top as its print
#      bed face, the roof away from it, to -Z.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-hole-entry-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-hole-entry'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.hole' = 'F7' } -StepPause 0

$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Whether a line containing the text was logged since the mark.
function Logged-SinceMark([string]$Text) {
  return [bool]((Ui-LogLines | Select-Object -Skip ($script:UiMark + 1)) | Where-Object { $_.Contains($Text) })
}

# The demo in a layout, Hole on the block's top face with its distances 15
# and 20 typed, the keyboard in Distance 2.
function Hole-OnTop([string]$Name, [string]$Chrome) {
  Write-Host "=== Hole, $Name"
  Ui-StartApp "hole-entry-$Name" @('--demo', "--chrome=$Chrome")
  Start-Sleep -Milliseconds 800
  Ui-Step 'fit (F6)'                     { Ui-Key 'F6' }
  Ui-Mark
  Ui-Step 'hole (F7)'                    { Ui-Key 'F7' }
  Ui-ExpectNew 'Command Hole started' 'the panel opened' 30
  Ui-ExpectNew 'Pick places:' 'the picks are logged'
  Ui-Mark
  Ui-Step 'click the top face'           { Ui-ClickPick "face F2.b0/F2:end($rect)" }
  Ui-ExpectNew 'Hole Place On: 1 face' 'the top face picked'
  Ui-ExpectNew 'Place Hole: face face F2:end(' 'measured from two of its edges'
  Ui-Mark
  Ui-Step 'distance 1: 15'               { Ui-TypeIn 'Panel Hole input at_offset1' '15' }
  Ui-ExpectNew 'Hole Distance 1: 15 = 15 mm' 'Distance 1 typed' 30
  Ui-Step 'distance 2: 20'               { Ui-TypeIn 'Panel Hole input at_offset2' '20' }
  Ui-ExpectNew 'Hole Distance 2: 20 = 20 mm' 'Distance 2 typed' 30
}

# Tab from Distance 2 to Diameter, which takes the typed 10; Shift+Tab back
# to Distance 2 and on to Distance 1.
function Tab-Steps {
  Write-Host '--- Tab and Shift+Tab between the fields'
  Ui-Mark
  $start = $script:UiMark
  Ui-Step 'Tab'                          { Ui-Key 'Tab' }
  Ui-ExpectNew 'Panel Hole Tab: at_offset2 -> diameter' 'Tab goes on from Distance 2 to Diameter'
  Ui-Step 'type 10'                      { Ui-Type '10' }
  Ui-ExpectNew 'Hole Diameter: 10 = 10 mm' 'Diameter took what was typed' 30
  Ui-Mark
  Ui-Step 'Shift+Tab'                    { Ui-Key 'shift+Tab' }
  Ui-ExpectNew 'Panel Hole Shift+Tab: diameter -> at_offset2' 'Shift+Tab back to Distance 2'
  Ui-Mark
  Ui-Step 'Shift+Tab again'              { Ui-Key 'shift+Tab' }
  Ui-ExpectNew 'Panel Hole Shift+Tab: at_offset2 -> at_offset1' 'and on to Distance 1'
  $script:UiMark = $start
  if ((Logged-SinceMark 'Command Hole cancelled') -or (Logged-SinceMark 'Done Hole')) { Ui-Fail 'Tab ended the command' }
  Write-Host 'ok   the command is still open'
}

Hole-OnTop 'floating' 'floating'
Tab-Steps

Write-Host '--- The dimensions in the view take typed values'
Ui-ExpectLog 'Value box Hole at_offset2 at ' 'Distance 2 is a dimension in the view' 30
Ui-ExpectLog 'Value box Hole depth at ' 'and Depth' 30
Ui-Mark
Ui-Step "Distance 2's value"             { Ui-ClickLogged 'Value box Hole at_offset2' }
Ui-ExpectNew 'Value box Hole at_offset2 opened: 20' 'a field over the view edits it'
Ui-Mark
Ui-Step 'type 25'                        { Ui-Type '25' }
Ui-Step 'Esc puts it back'               { Ui-Key 'Escape' }
Ui-ExpectNew 'Value box Hole at_offset2 closed' 'the box closed'
Ui-ExpectNew 'Hole Distance 2: 20' 'Distance 2 is 20 again' 30
if (Logged-SinceMark 'Command Hole cancelled') { Ui-Fail 'Esc in the box cancelled the command' }

Ui-Mark
Ui-Step "Depth's value"                  { Ui-ClickLogged 'Value box Hole depth' }
Ui-ExpectNew 'Value box Hole depth opened: 10' 'a field over the view edits the depth'
Ui-Step 'type 12'                        { Ui-Type '12' }
Ui-Step 'Enter keeps it'                 { Ui-Key 'Return' }
Ui-ExpectNew 'Value box Hole depth closed' 'the box closed'
Ui-ExpectNew 'Hole Depth: 12 mm = 12 mm' 'Depth is 12' 30
Ui-ExpectNew 'Preview Hole: ok' 'the preview followed' 30
if ((Logged-SinceMark 'Done Hole') -or (Logged-SinceMark 'Added 1 hole')) { Ui-Fail 'Enter in the box ended the command' }

Ui-Mark
Ui-Step "Depth's value again"            { Ui-ClickLogged 'Value box Hole depth' }
Ui-ExpectNew 'Value box Hole depth opened: 12' 'the box opens with 12'
Ui-Step 'type abc'                       { Ui-Type 'abc' }
Ui-ExpectNew 'Value box Hole depth invalid' 'the box shows the error' 30
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew 'Value box Hole depth refused: abc' 'Enter refused'
Ui-Sync
if ((Logged-SinceMark 'Value box Hole depth closed') -or (Logged-SinceMark 'Done Hole') -or
    (Logged-SinceMark 'Added 1 hole')) {
  Ui-Fail 'Enter closed the box or ended the command with abc'
}
Write-Host 'ok   the box stays open'
Ui-Mark
Ui-Step 'Esc puts 12 back'               { Ui-Key 'Escape' }
Ui-ExpectNew 'Value box Hole depth closed' 'the box closed'
Ui-ExpectNew 'Hole Depth: 12' 'Depth is 12 again' 30

Ui-Mark
Ui-Step "Depth's value once more"        { Ui-ClickLogged 'Value box Hole depth' }
Ui-ExpectNew 'Value box Hole depth opened: 12' 'the box opens with 12'
Ui-Step 'type 10'                        { Ui-Type '10' }
Ui-Step 'Enter keeps it'                 { Ui-Key 'Return' }
Ui-ExpectNew 'Value box Hole depth closed' 'the box closed'
Ui-ExpectNew 'Hole Depth: 10 mm = 10 mm' 'Depth is 10' 30
Ui-ExpectNew 'Preview Hole: ok' 'previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added 1 hole(s) (simple), diameter 10' 'the hole added' 30
# The hole of tools/ui-create-test.sh (hole_simple_distance).
Ui-ExpectVolume 'Body Body1 \(F2\.b0\): volume 48000\.000 -> ([0-9.]+) mm3' 47135.94936549554 'one hole, 10 deep'

Hole-OnTop 'docked' 'docked'
Tab-Steps
Ui-Step 'Esc (cancel)'                   { Ui-Key 'Escape' }

Write-Host '=== A reference picked in its row (mitcad#1285)'
# The sketch line of a top edge (c1 along X at y 0, c2 at x 60, c3 at y 40,
# c4 at x 0), the one across the face from it, and the face's center's
# distance from it.
function Side-Of([string]$Edge) {
  if ($Edge -notmatch 'F2:side\((c[1-4])\[') { Ui-Fail "no side in $Edge" }
  return $Matches[1]
}
$tokens = @{ 'c1' = 'c1[c4,c2]'; 'c2' = 'c2[c1,c3]'; 'c3' = 'c3[c2,c4]'; 'c4' = 'c4[c3,c1]' }
$opposites = @{ 'c1' = $tokens['c3']; 'c2' = $tokens['c4']; 'c3' = $tokens['c1']; 'c4' = $tokens['c2'] }
function Center-From([string]$Side) { if ($Side -eq 'c1' -or $Side -eq 'c3') { return 20 } else { return 30 } }
# The top edge of the side 'cN[neighbours]' among the newest pick places;
# null when it cannot be clicked (behind the panel, say).
function Top-Edge([string]$Side) {
  $edge = $null
  foreach ($line in @(Ui-LogLines)) {
    if ($line.StartsWith('Pick places:')) {
      $edge = $null
    } elseif ($line -match '^Pick edge F2\.b0/(E\{\S+\}) at ' -and $line.Contains('F2:end(') -and
              $line.Contains("F2:side($Side)")) {
      $edge = $Matches[1]
    }
  }
  return $edge
}
# Whether the panel said it failed since the mark.
function Assert-NoFailure {
  $failed = (Ui-LogLines | Select-Object -Skip ($script:UiMark + 1)) | Where-Object { $_.StartsWith('Panel Hole error: ') }
  if ($failed) { Ui-Fail "a failure notice: $(@($failed)[-1])" }
  Write-Host 'ok   no failure notice'
}
Ui-StartApp 'hole-entry-reference' @('--demo', '--chrome=floating')
Start-Sleep -Milliseconds 800
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'hole (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Hole started' 'the panel opened' 30
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-Mark
Ui-Step 'click the top face'             { Ui-ClickPick "face F2.b0/F2:end($rect)" }
Ui-ExpectNew 'Hole Place On: 1 face' 'the top face picked'
Ui-ExpectNew 'Place Hole: face face F2:end(' 'measured from two of its edges'
$place = Ui-LastMatch '^(Place Hole: face face .*)$'
if ("$place" -notmatch '^Place Hole: face face \S+ of F2\.b0, [0-9.-]+ mm from edge (E\{\S+\}) of F2\.b0, [0-9.-]+ mm from edge (E\{\S+\}) of F2\.b0 \(point') {
  Ui-Fail "no references logged: $place"
}
$ref1 = $Matches[1]
$ref2 = $Matches[2]
$side1 = Side-Of $ref1
$side2 = Side-Of $ref2
$opposite = $opposites[$side2]
$d1 = Center-From $side1
$d2 = Center-From $side2
Write-Host "     references $side1 and ${side2}: the face's center $d1 and $d2 from them"
Ui-Mark
Ui-Step "distance 1: $d1"                { Ui-TypeIn 'Panel Hole input at_offset1' "$d1" }
Ui-ExpectNew "Hole Distance 1: $d1 = $d1 mm" 'Distance 1 typed' 30
Ui-Step "distance 2: $d2"                { Ui-TypeIn 'Panel Hole input at_offset2' "$d2" }
Ui-ExpectNew "Hole Distance 2: $d2 = $d2 mm" 'Distance 2 typed' 30
Ui-Step 'diameter 10'                    { Ui-TypeIn 'Panel Hole input diameter' '10' }
Ui-ExpectNew 'Hole Diameter: 10 = 10 mm' 'the diameter typed' 30
Ui-ExpectNew 'Preview Hole: ok' "the hole previewed at the face's center" 30
Ui-Step 'the Reference 2 row'            { Ui-ClickLogged 'Panel Hole input at_edge2' }
$other = Top-Edge $opposite
if (-not $other) { Ui-Fail 'no pick place logged for the edge across from Reference 2' }
# For the second pick: the edge across from Reference 1, or else Reference
# 2's first edge, whichever the view lets a click pick.
$across1 = Top-Edge $opposites[$side1]
$first2 = Top-Edge $tokens[$side2]
Ui-Mark
Ui-Step 'click the edge across the face' { Ui-ClickPick "edge F2.b0/$other at" }
Ui-ExpectNew "Hole Reference 2: 1 edge [edge $other of F2.b0]" 'the edge across the face is Reference 2'
Ui-ExpectNew ", $d2 mm from edge $other of F2.b0 (point 30, 20, 20)" "the hole kept at the center, $d2 mm from the new edge"
Ui-ExpectNew "Hole Distance 1: $d1 mm = $d1 mm" 'Distance 1 evaluated' 30
Ui-ExpectNew "Hole Distance 2: $d2 mm = $d2 mm" 'Distance 2 evaluated' 30
Ui-ExpectNew 'Preview Hole: ok' 'previewed where it was' 30
Assert-NoFailure

# Off the center: 12 mm from a row's reference, then the edge across from
# it picked through that row measures the rest of the face's width.
if ($across1) {
  $row = 1; $now = $side1; $edge = $across1
} elseif ($first2) {
  $row = 2; $now = $opposite.Substring(0, 2); $edge = $first2
} else {
  Ui-Fail "neither the edge across from Reference 1 nor Reference 2's first edge can be picked"
}
$across = @{ 'c1' = @(40, '30, 12, 20'); 'c3' = @(40, '30, 28, 20'); 'c2' = @(60, '48, 20, 20'); 'c4' = @(60, '12, 20, 20') }
$width = $across[$now][0]
$at = $across[$now][1]
$rest = $width - 12
Ui-Mark
Ui-Step "distance ${row}: 12"            { Ui-TypeIn "Panel Hole input at_offset$row" '12' }
Ui-ExpectNew "Hole Distance ${row}: 12 = 12 mm" "Distance $row typed" 30
Ui-ExpectNew 'Preview Hole: ok' "previewed 12 mm from Reference $row" 30
Ui-Step "the Reference $row row"         { Ui-ClickLogged "Panel Hole input at_edge$row" }
Ui-Mark
Ui-Step 'click the edge across from it'  { Ui-ClickPick "edge F2.b0/$edge at" }
Ui-ExpectNew "Hole Reference ${row}: 1 edge [edge $edge of F2.b0]" "the edge across the face is Reference $row"
Ui-ExpectNew "$rest mm from edge $edge of F2.b0" "$rest mm from the new edge"
$placed = Ui-LastMatch '^(Place Hole: .*)$'
if (-not "$placed".Contains("(point $at)")) { Ui-Fail "the hole moved: $placed" }
Write-Host "ok   the hole kept at $at"
Ui-ExpectNew "Hole Distance ${row}: $rest mm = $rest mm" "Distance $row evaluated" 30
Ui-ExpectNew 'Preview Hole: ok' 'previewed where it was' 30
Assert-NoFailure
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added 1 hole(s) (simple), diameter 10' 'the hole added' 30
Ui-ExpectVolume 'Body Body1 \(F2\.b0\): volume 48000\.000 -> ([0-9.]+) mm3' 47135.94936549554 'one hole, 10 deep'

Write-Host '=== Print Shape with and without a print bed face (mitcad#1744)'
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# The block with a 6 mm hole lying in it, along +Y from its side at y 0;
# a copy with the block's top as its print bed face.
$lying = @(
  '{"cmd": "sketch.create"}',
  '{"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}',
  ('{"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect +
   '"}], "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}'),
  ('{"cmd": "add_feature", "def": {"type": "hole", "diameter": 6, "extent": {"type": "distance", "depth": 10}, ' +
   '"placement": {"type": "plane", "plane": {"origin": [0, 0, 0], "normal": [0, -1, 0]}, "points": [[30, 0, 10]]}}}'))
$bedFace = '{"cmd": "set_print_bed_face", "uid": "F2.b0", "face": "F2:end(' + $rect + ')"}'
$designs = @{ 'free' = $lying; 'bed' = $lying + $bedFace }
foreach ($name in $designs.Keys) {
  $scriptFile = Join-Path $work "$name.json"
  Set-Content -Encoding ascii $scriptFile ('[' + ($designs[$name] -join ', ') + ']')
  $made = & $cli run $scriptFile --save (Join-Path $work "$name.mitcad")
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli run ${name}: $($made -join ' / ')" }
}

# The block's planar faces whose outward normal is (x, 0, z), the number
# expected. A teardrop's roof along Y has two sides at 45 degrees; their
# normals point back into the hole, down from a roof that points up.
function Roof-Faces([int]$X, [int]$Z, [int]$Count) {
  return ('{"expect": {"query": {"query": "faces", "body": "F2.b0", "normal": [' + $X + ', 0, ' + $Z +
          '], "normal_tolerance": 0.001}, "result": {"total": ' + $Count + '}}}')
}
# The expectations hold for the saved design.
function Check-Design([string]$Description, [string]$Design, [string[]]$Expectations) {
  $scriptFile = Join-Path $work 'check.json'
  Set-Content -Encoding ascii $scriptFile ('[' + ($Expectations -join ', ') + ']')
  $checked = & $cli run $scriptFile --open $Design
  if ($LASTEXITCODE -ne 0) { Ui-Fail "${Description}: $($checked -join ' / ')" }
  Write-Host "ok   $Description"
}
$teardropSaved = @(
  '{"expect": {"feature": "F3", "status": "ok"}}',
  '{"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {"def": {"print_shape": "teardrop"}}}}')

# Hole1 of the design edited to a Teardrop and saved.
function Edit-Teardrop([string]$Name) {
  Ui-StartApp "hole-entry-print-$Name" @('--open', (Join-Path $work "$Name.mitcad"))
  Ui-Step 'fit (F6)'                     { Ui-Key 'F6' }
  Ui-ExpectLog 'Bodies shown: F2.b0' 'the design shown'
  Ui-Mark
  Ui-Step 'edit Hole1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Hole1' }
  Ui-ExpectNew 'Editing F3 with Hole' 'Hole1 opens for editing' 30
  Ui-ExpectNew 'Hole placement kept: plane' 'its plane kept as it is'
  Ui-ExpectNew 'Panel Hole input print_shape at' 'Print Shape listed'
  $row = Ui-LastMatch '^(Panel Hole input print_shape at .*)$'
  if ("$row".Contains('(hidden)')) { Ui-Fail 'Print Shape is hidden' }
  Write-Host 'ok   Print Shape shown'
  Ui-Mark
  Ui-Step 'Teardrop (wheel)'             { Ui-WheelLogged 'Panel Hole input print_shape' -1 }
  Ui-ExpectNew 'Hole: Print Shape = Teardrop' 'Teardrop chosen'
  Ui-ExpectNew 'Preview Hole: ok' 'previewed' 30
  Ui-Mark
  Ui-Step 'OK (Enter)'                   { Ui-Key 'Return' }
  Ui-ExpectNew 'Edited F3' 'Hole1 edited' 30
  if (Logged-SinceMark 'Recompute failed') { Ui-Fail 'a recompute failed' }
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                { Ui-Key 'ctrl+s' }
  Ui-ExpectNew 'Saved ' 'saved'
  Ui-StopApp
}

Write-Host '--- Without a print bed face: the design''s -Z is down'
Edit-Teardrop 'free'
Check-Design 'a teardrop without a print bed face, its roof toward +Z' (Join-Path $work 'free.mitcad') `
  ($teardropSaved + @((Roof-Faces 1 -1 1), (Roof-Faces -1 -1 1), (Roof-Faces 1 1 0), (Roof-Faces -1 1 0)))

Write-Host '--- With the block''s top as its print bed face'
Edit-Teardrop 'bed'
Check-Design 'a teardrop with the top as the print bed face, its roof away from it, toward -Z' (Join-Path $work 'bed.mitcad') `
  ($teardropSaved + @((Roof-Faces 1 1 1), (Roof-Faces -1 1 1), (Roof-Faces 1 -1 0), (Roof-Faces -1 -1 0)))

Ui-Finish 'UI Windows Hole entry test'
