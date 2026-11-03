# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/commands/HoleCommands.cpp app/commands/PlaceInputs.cpp
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

Ui-Finish 'UI Windows Hole entry test'
