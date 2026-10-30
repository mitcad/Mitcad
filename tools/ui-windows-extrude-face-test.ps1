# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/Command.hpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/extrude.rs core/ffi/src/kernel/face_extrude.rs
# check-all sources: core/cpp/bridge/face_extrude.cpp geometry/src/face_extrude.cpp tools/cli
# check-all sources: app/commands/SketchNormal.cpp app/framework/ManipulatorOverlay.cpp
# Input-driven Windows UI test of Extrude with a planar body face as the
# profile (mitcad#535) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), as tools/ui-extrude-face-test.sh on Linux (whose
# context menu part needs a popup, which session 0 has not):
#   1. One imported 20 mm cube (written to STEP and read back, no sketch):
#      Extrude starts with no profile, a face picked in the view is the
#      profile, Join chosen automatically, Two Sides takes a negative
#      Distance 2 that ends on the other side (mitcad#765) and refuses ends
#      that meet, 5 mm out: 8000 -> 10000 mm3.
#   2. A 20 mm cube from a sketch (Body1, F2.b0) beside an unused square
#      sketch: its side x = 20 selected before Extrude is the profile, not
#      the square; 5 mm into the cube is Cut, chosen automatically:
#      8000 -> 6000 mm3.
#   3. A square sketch in component Brace, whose occurrence is turned a
#      quarter about X (mitcad#958): Extrude's distance arrow points along
#      the sketch's normal in the design (-Y), not the component's +Z.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-face-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-face'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$r = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$right = 'F2:side(c2[c1,c3])'
# A path the JSON scripts can hold as it is.
$step = (Join-Path $work 'cube.step').Replace('\', '/')

$cubeJson = Join-Path $work 'cube.json'
Set-Content -Encoding ascii $cubeJson (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"expect": {"body": "Body1", "volume": 8000, "tolerance": 1e-9}},
  {"cmd": "export", "path": "STEP"}
]
'@).Replace('RECT', $r).Replace('STEP', $step)
$importedJson = Join-Path $work 'imported.json'
Set-Content -Encoding ascii $importedJson (@'
[
  {"cmd": "import_file", "path": "STEP"},
  {"expect": {"feature": "Base1", "status": "ok", "volume": 8000, "tolerance": 1e-6}}
]
'@).Replace('STEP', $step)
$blockJson = Join-Path $work 'block.json'
Set-Content -Encoding ascii $blockJson (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-40, 0], "width": 10, "height": 10}
]
'@).Replace('RECT', $r)
$turnedJson = Join-Path $work 'turned.json'
Set-Content -Encoding ascii $turnedJson (@'
[
  {"cmd": "create_component", "name": "Brace", "transform": [[1, 0, 0, 0], [0, 0, -1, 0], [0, 1, 0, 30]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20}
]
'@)
$imported = Join-Path $work 'imported.mitcad'
$block = Join-Path $work 'block.mitcad'
$turned = Join-Path $work 'turned.mitcad'
foreach ($run in @(@($cubeJson, (Join-Path $work 'cube.mitcad')), @($importedJson, $imported), @($blockJson, $block),
                   @($turnedJson, $turned))) {
  $output = & $cli run $run[0] --save $run[1] 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($run[0]): $($output -join ' / ')" }
}

# Expect-NoNew "text" "description": no such line since the mark.
function Expect-NoNew([string]$Text, [string]$Description) {
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains($Text) }) {
    Ui-Fail $Description
  }
  Write-Host "ok   $Description"
}

Write-Host '--- 1. A face of an imported body, picked in the command'
Ui-StartApp 'extrude-face-imported' @('--open', $imported)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the imported cube shown'
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started in a design without sketches'
Ui-ExpectNew 'Extrude Profiles: nothing' 'no profile to take'
Ui-ExpectNew 'Pick places:' 'its Profiles take faces'
Ui-Mark
Ui-Step 'pick a face of the cube'        { Ui-ClickPick 'face F1.b0/F1:import(' }
Ui-ExpectNew 'Extrude Profiles: 1 face [face F1:import(' 'the face is the profile'
Ui-ExpectNew 'Extrude: automatic Operation = Join' 'Join: out of the cube' 30
# Two Sides takes a negative side, which ends on the other side's side
# (mitcad#765): 10 and -3 preview; 10 and -10 meet and are refused; back
# to One Side.
Ui-Mark
Ui-Step 'Two Sides (wheel)'              { Ui-WheelLogged 'Panel Extrude input direction' -1 }
Ui-ExpectNew 'Extrude: Direction = Two Sides' 'two sides chosen' 30
Ui-Mark
Ui-Step 'side two -10'                   { Ui-TypeIn 'Panel Extrude input distance2' '-10' }
Ui-ExpectNew 'Panel Extrude error: The two sides overlap: Distance and Distance 2 must add up to more than zero (a negative distance ends on the other side)' 'ends that meet refused' 30
Ui-Mark
Ui-Step 'side two -3'                    { Ui-TypeIn 'Panel Extrude input distance2' '-3' }
Ui-ExpectNew 'Preview Extrude: ok' 'a negative side two previewed' 30
Ui-Mark
Ui-Step 'One Side (wheel)'               { Ui-WheelLogged 'Panel Extrude input direction' 1 }
Ui-ExpectNew 'Extrude: Direction = One Side' 'one side again' 30
Ui-Mark
Ui-Step 'distance 5'                     { Ui-TypeIn 'Panel Extrude input distance' '5' }
Ui-ExpectNew 'Preview Extrude: ok' 'previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added extrude (Join), distance 5, of a face' 'the face extruded' 30
Ui-ExpectVolume 'Body [^(]* \(F1\.b0\): volume 8000\.000 -> ([0-9.]+) mm3' 10000 'the cube 5 mm longer'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-StopApp

Write-Host '--- 2. The side selected before Extrude, beside an unused square'
Ui-StartApp 'extrude-face-block' @('--open', $block)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the cube shown'
# Extrude once for the log's pick places (it takes the square), cancelled.
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew "Extrude Profiles: 1 profile [profile $r of F3" 'with nothing selected, the square'
Ui-ExpectNew 'Pick places:' 'where the faces are picked'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'cancelled'
Ui-Mark
Ui-Step 'select the side x = 20'         { Ui-ClickPick "face F2.b0/$right" }
Ui-ExpectNew "Selected: 1 face [face $right of F2.b0" 'the side selected'
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew "Extrude Profiles: 1 face [face $right of F2.b0" 'the side is the profile'
Expect-NoNew 'Extrude Profiles: 1 profile' 'the unused square does not stand in'
Ui-Mark
Ui-Step 'distance -5'                    { Ui-TypeIn 'Panel Extrude input distance' '-5' }
Ui-ExpectNew 'Extrude: automatic Operation = Cut' 'Cut: into the cube' 30
Ui-ExpectNew 'Preview Extrude: ok' 'the cut previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added extrude (Cut), distance -5, of a face' 'the side cut in' 30
Ui-ExpectVolume 'Body Body1 \(F2\.b0\): volume 8000\.000 -> ([0-9.]+) mm3' 6000 'the cube 15 wide'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-StopApp

Write-Host '--- 3. A sketch in a turned occurrence: the arrow along its normal in the design (mitcad#958)'
Ui-StartApp 'extrude-face-turned' @('--open', $turned)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew "Extrude Profiles: 1 profile [profile $r of F1" 'the square in Brace is the profile'
Ui-ExpectNew 'Manipulator Extrude distance at ' "the distance's arrow" 30
# Brace:1 turns the component's +Z to the design's -Y.
Ui-ExpectNew 'Arrow direction Extrude distance: 0.000,-1.000,0.000' "the arrow along the sketch's normal in the design" 30
Expect-NoNew 'Arrow direction Extrude distance: 0.000,0.000,1.000' "not along the component's own normal"
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'cancelled'

Ui-Finish 'UI Windows extrude face test'
