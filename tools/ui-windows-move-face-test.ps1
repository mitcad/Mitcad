# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveFaceText.cpp app/commands/MoveFaceText.hpp
# check-all sources: app/MainWindowMoveFace.cpp app/framework/CommandPanel.cpp core/model/src/features/move_face.rs
# check-all sources: geometry/src/move_faces.cpp tools/cli
# Input-driven Windows UI test of MODIFY > Move Face (mitcad#531) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-move-face-test.sh on Linux, on a 60 x 40 x 20 block mitcad-cli
# makes (Sketch1 F1, Extrude1 F2, Body1):
#   1. The end face at x = 60 pushed out 5 mm (Translate): previewed, the
#      panel's note naming it; placed, 52000 mm3.
#   2. Its timeline cell: the selection card names it; a double-click edits
#      it with its face and motion, 10 mm: 56000 mm3.
#   3. The top tilted 5 degrees about its front edge (Rotate): 70 x (800 +-
#      800 tan 5 deg) mm3, as the edge's direction runs.
#   4. The top pushed down 30 mm: the model refuses it, the panel says why
#      in red, OK is refused.
# Move Face has a shortcut of the test's own (F12): the command search's
# popup closes at once in session 0; Move Type is chosen with the wheel.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-move-face-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-face'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.move_face' = 'F12' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$r = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$top = "F2:end($r)"
$front = 'F2:side(c1[c4,c2])'
$right = 'F2:side(c2[c1,c3])'
$topFront = "E{$top|$front}"

$json = Join-Path $work 'block.json'
Set-Content -Encoding ascii $json (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"expect": {"body": "Body1", "volume": 48000, "tolerance": 1e-9}}
]
'@).Replace('RECT', $r)
$design = Join-Path $work 'block.mitcad'
$output = & $cli run $json --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# The volume after a change: the last "Body Body1 (F2.b0): volume <from> -> <v> mm3".
function Volume-After([string]$From) {
  for ($i = 0; $i -lt 50; $i++) {
    $volume = Ui-LastMatch ('Body Body1 \(F2\.b0\): volume ' + $From + ' -> ([0-9.]+) mm3')
    if ($volume) { return [double]::Parse($volume, [Globalization.CultureInfo]::InvariantCulture) }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "no volume logged after $From mm3"
}
function Expect-Volume([string]$From, [double]$Expected, [string]$Description) {
  $v = Volume-After $From
  if ([Math]::Abs($v - $Expected) -gt 1e-2) { Ui-Fail "${Description}: volume $v mm3, expected $Expected" }
  Write-Host "ok   ${Description}: $v mm3"
}
# Move Type from Free Move, by wheel steps down the list (a step up is the
# entry above): Translate 1, Rotate 3. A combo box moves one entry per
# wheel message whatever its delta, so one message a step, 150 ms apart
# (as Wheel-Choice in tools/ui-windows-build-test.ps1).
function Choose-Type([int]$Index) {
  for ($i = 0; $i -lt $Index; $i++) {
    Ui-WheelLogged 'Panel Move Face input type' -1
    Start-Sleep -Milliseconds 150
  }
}

Ui-StartApp 'move-face' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }

Write-Host '--- 1. The end face pushed out 5 mm'
Ui-Mark
Ui-Step 'move face (F12)'                 { Ui-Key 'F12' }
Ui-ExpectNew 'Command Move Face started' 'Move Face started'
Ui-ExpectNew 'Pick places:' 'it logs where its faces are'
Ui-Step 'pick the end face at x = 60'    { Ui-ClickPick "face F2.b0/$right" }
Ui-ExpectLog "Move Face Faces: 1 face [face $right of F2.b0]" 'the end face picked'
Ui-Step 'Translate'                      { Choose-Type 1 }
Ui-ExpectLog 'Move Face: Move Type = Translate' 'Translate chosen'
Ui-Mark
Ui-Step 'X 5'                            { Ui-TypeIn 'Panel Move Face input x' '5' }
Ui-ExpectNew 'Preview Move Face: ok' 'previewed'
Ui-ExpectNew 'Panel Move Face note: Move Face: 1 face of Body1, translate (5, 0, 0) mm' 'the note names it'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Moved faces: Move Face: 1 face of Body1, translate (5, 0, 0) mm' 'placed'
Expect-Volume '48000\.000' 52000 'the block 65 long'

Write-Host '--- 2. Its timeline cell: the card, then an edit'
Ui-Mark
Ui-Step 'click MoveFace1 in the timeline' { Ui-ClickLogged 'Timeline MoveFace1' }
Ui-ExpectNew 'Move Face card: MoveFace1: 1 face of Body1, translate (5, 0, 0) mm' 'the selection card names it'
Ui-Mark
Ui-Step 'double-click MoveFace1'         { Ui-DoubleClickLogged 'Timeline MoveFace1' }
Ui-ExpectNew 'Editing F3 with Move Face' 'the same panel edits it'
Ui-ExpectNew "Move Face Faces: 1 face [face $right of F2.b0]" 'with its face'
Ui-ExpectNew 'Move Face X Distance: 5' 'and its motion'
Ui-Step 'X 10'                           { Ui-TypeIn 'Panel Move Face input x' '10' }
Ui-ExpectNew 'Panel Move Face note: Move Face: 1 face of Body1, translate (10, 0, 0) mm' 'previewed at 10 mm'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F3' 'edited'
Expect-Volume '[0-9.]+' 56000 'the block 70 long'

Write-Host '--- 3. The top tilted 5 degrees about its front edge'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'move face (F12)'                 { Ui-Key 'F12' }
Ui-ExpectNew 'Pick places:' 'it logs where its faces are'
Ui-Step 'pick the top'                   { Ui-ClickPick "face F2.b0/$top" }
Ui-ExpectLog "Move Face Faces: 1 face [face $top of F2.b0]" 'the top picked'
Ui-Step 'Rotate'                         { Choose-Type 3 }
Ui-ExpectLog 'Move Face: Move Type = Rotate' 'Rotate chosen'
Ui-Mark
Ui-Step 'the axis input'                 { Ui-ClickLogged 'Panel Move Face input axis' }
Ui-ExpectNew 'Pick places:' 'the axis input logs its edges'
Ui-Step "pick the top's front edge"      { Ui-ClickPick "edge F2.b0/$topFront" }
Ui-ExpectLog "Move Face Axis: 1 edge [edge $topFront of F2.b0]" 'the front edge is the axis'
Ui-Mark
Ui-Step '5 degrees'                      { Ui-TypeIn 'Panel Move Face input angle' '5' }
Ui-ExpectNew 'Preview Move Face: ok' 'previewed'
Ui-ExpectNew 'Panel Move Face note: Move Face: 1 face of Body1, rotate 5 deg about an edge' 'the note names it'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Moved faces: ' 'placed'
$tan = [Math]::Tan(5.0 * [Math]::PI / 180.0)
$v = Volume-After '56000\.000'
$up = 70.0 * (800.0 + 800.0 * $tan)
$down = 70.0 * (800.0 - 800.0 * $tan)
if ([Math]::Abs($v - $up) -gt 1e-2 -and [Math]::Abs($v - $down) -gt 1e-2) {
  Ui-Fail "the tilted top: volume $v mm3, expected $up or $down"
}
Write-Host "ok   the top tilted about its front edge: $v mm3"

Write-Host '--- 4. A motion the neighbours cannot follow'
Ui-Mark
Ui-Step 'move face (F12)'                 { Ui-Key 'F12' }
Ui-ExpectNew 'Pick places:' 'it logs where its faces are'
Ui-Step 'pick the top'                   { Ui-ClickPick "face F2.b0/$top" }
Ui-Step 'Translate'                      { Choose-Type 1 }
Ui-Mark
Ui-Step 'Z -30 (below the bottom)'       { Ui-TypeIn 'Panel Move Face input z' '-30' }
Ui-ExpectNew 'Preview Move Face: failed: ' 'the model refuses it'
Ui-ExpectNew 'Panel Move Face error: ' 'the panel says why in red'
Ui-Mark
Ui-Step 'Enter (refused)'                { Ui-Key 'Return' }
if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.StartsWith('Moved faces: ') }) {
  Ui-Fail 'OK placed a motion the model refused'
}
Write-Host 'ok   OK is refused'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Ui-Finish 'UI Windows Move Face test'
