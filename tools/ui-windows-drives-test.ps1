# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DriveCommands.cpp app/commands/PlaceInputs.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandSession.cpp core/model/src/features/drives.rs
# check-all sources: core/model/src/features/place_at.rs core/model-api/src/drives.rs tools/cli
# Input-driven Windows UI test of Gear Pair and Belt Drive (mitcad#656) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted); on
# Linux the same steps are the last parts of tools/ui-gear-test.sh and
# tools/ui-pulley-test.sh. A design mitcad-cli makes: two 8 mm shafts 30
# high, 60 mm apart (Cylinder1 F1.b0 at the origin, Cylinder2 F2.b0 at
# (60, 0)):
#   1. Gear Pair: the shafts' top edges picked as the two axes, ratio 2:
#      the rows list the gears that fit, the exact pairs needing no
#      correction against undercut with the fewest teeth first (2 mm, 20
#      and 40 teeth: 2 x 60 / 2 = 60; 2.5 mm with 16 teeth, corrected,
#      after them); the second row (module 1.25, 32 and 64 teeth)
#      previewed as new bodies; a Second Offset of 5 mm keeps the second
#      row chosen (mitcad#761); OK makes both gears.
#   2. Undo, then Belt Drive between the same shafts, ratio 2, GT2 2 mm:
#      the rows list the tooth counts with their belts; the first
#      previewed as new bodies, its belt as values; OK makes both pulleys.
#   3. Gear Pair with a shaft in another component's occurrence
#      (mitcad#754), in a second design: an 8 mm shaft in Motor:1 (F1.b0,
#      the occurrence 60 mm along X) and one in the root at the origin
#      (F2.b0). The root's shaft first and the motor's second is refused
#      with the reason (the second gear meshes, so it goes on a shaft of
#      the active component); the motor's shaft first and the root's
#      second list the gears for the axes 60 mm apart; OK makes both.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drives-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drives'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.gear_pair' = 'F7'; 'solid.belt_drive' = 'F8' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$shaftsJson = Join-Path $work 'shafts.json'
Set-Content -Encoding ascii $shaftsJson @'
[
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [60, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}}
]
'@
$shafts = Join-Path $work 'shafts.mitcad'
$output = & $cli run $shaftsJson --save $shafts 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
$motorJson = Join-Path $work 'motor.json'
Set-Content -Encoding ascii $motorJson @'
[
  {"cmd": "create_component", "name": "Motor", "transform": {"translation": [60, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}}
]
'@
$motor = Join-Path $work 'motor.mitcad'
$output = & $cli run $motorJson --save $motor 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

Ui-StartApp 'drives' @('--open', $shafts)
Start-Sleep -Milliseconds 800
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0, F2.b0' 'the shafts shown'

Write-Host '--- 1. Gear Pair between the shafts'
Ui-Mark
Ui-Step 'gear pair (F7)'                 { Ui-Key 'F7' }
Ui-ExpectNew 'Command Gear Pair started' 'the panel opened' 30
Ui-ExpectNew 'Pick edge F1.b0/E{F1:side0|F1:top} at' "the first shaft's top edge can be picked"
Ui-Mark
Ui-Step 'the first shaft'                { Ui-ClickPick 'edge F1.b0/E{F1:side0|F1:top} at' }
Ui-ExpectNew 'Gear Pair First Axis: 1 edge' 'the first axis picked'
Ui-ExpectNew 'Pick edge F2.b0/E{F2:side0|F2:top} at' "the second shaft's top edge can be picked"
Ui-Mark
Ui-Step 'the second shaft'               { Ui-ClickPick 'edge F2.b0/E{F2:side0|F2:top} at' }
Ui-ExpectNew 'Gear Pair candidates: ' "the model's candidates" 30
Ui-ExpectNew 'center distance 60 mm' 'the axes 60 mm apart'
Ui-ExpectNew 'Panel Gear Pair options candidate: 2 mm, 20:40' 'no correction and the fewest teeth first' 30
Ui-Mark
# One step towards the far side: New Body after New Component.
Ui-Step 'operation: New Body'            { Ui-WheelLogged 'Panel Gear Pair input operation' -1 }
Ui-ExpectNew 'Gear Pair: Operation = New Body' 'as new bodies'
Ui-ExpectNew 'Preview Gear Pair: ok' 'the pair previewed' 30
Ui-Mark
Ui-Step 'the second row'                 { Ui-ClickLogged 'Panel Gear Pair input candidate=1' }
Ui-ExpectNew 'Gear Pair: Gears = 1.25 mm' 'module 1.25 chosen'
Ui-ExpectNew 'Preview Gear Pair: ok' 'module 1.25 previewed' 30
# An Offset asks for the candidates again: the second row stays chosen
# (mitcad#761).
Ui-Mark
Ui-Step 'More'                           { Ui-ClickLogged 'Panel Gear Pair input drive_more' }
Ui-ExpectNew 'Panel Gear Pair input second_offset at' 'the Second Offset shown'
# The panel's layout settled before the field is clicked, as on Linux.
Ui-Sync
Ui-Mark
Ui-Step 'Second Offset 5'                { Ui-TypeIn 'Panel Gear Pair input second_offset' '5' }
Ui-ExpectNew 'Gear Pair candidates: ' 'the candidates again for the offset' 30
Ui-ExpectNew 'Gear Pair candidate kept: row 2' 'the second row still chosen'
Ui-ExpectNew 'Preview Gear Pair: ok' 'the offset previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added gear pair, module 1.25, 32 and 64 teeth (exact)' 'both gears added' 30

Write-Host '--- 2. Belt Drive between the same shafts'
Ui-Mark
Ui-Step 'undo the gears (Ctrl+Z)'        { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Bodies shown: F1.b0, F2.b0' 'the gears gone' 30
Ui-Mark
Ui-Step 'belt drive (F8)'                { Ui-Key 'F8' }
Ui-ExpectNew 'Command Belt Drive started' 'the panel opened' 30
Ui-ExpectNew 'Pick edge F1.b0/E{F1:side0|F1:top} at' "the first shaft's top edge can be picked"
Ui-Mark
Ui-Step 'the first shaft'                { Ui-ClickPick 'edge F1.b0/E{F1:side0|F1:top} at' }
Ui-ExpectNew 'Pick edge F2.b0/E{F2:side0|F2:top} at' "the second shaft's top edge can be picked"
Ui-Mark
Ui-Step 'the second shaft'               { Ui-ClickPick 'edge F2.b0/E{F2:side0|F2:top} at' }
Ui-ExpectNew 'Belt Drive candidates: ' "the model's candidates" 30
Ui-ExpectNew 'Panel Belt Drive options candidate: ' 'the rows listed' 30
Ui-Mark
Ui-Step 'operation: New Body'            { Ui-WheelLogged 'Panel Belt Drive input operation' -1 }
Ui-ExpectNew 'Preview Belt Drive: ok' 'the drive previewed' 30
Ui-ExpectNew 'Panel Belt Drive note: Belt length ' 'the belt as values' 30
$note = Ui-LastMatch '(Panel Belt Drive note: .*)'
Write-Host "     $note"
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added belt drive (gt2_2mm), ' 'both pulleys added' 30

Write-Host "--- 3. Gear Pair with a shaft in another component's occurrence (mitcad#754)"
Ui-StartApp 'drives-motor' @('--open', $motor)
Start-Sleep -Milliseconds 800
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Body F1.b0 in O1 at ' "the motor's shaft shown in its occurrence"
Ui-Mark
Ui-Step 'gear pair (F7)'                 { Ui-Key 'F7' }
Ui-ExpectNew 'Command Gear Pair started' 'the panel opened' 30
Ui-ExpectNew 'Pick edge F2.b0/E{F2:side0|F2:top} at' "the root's shaft top edge can be picked"
Ui-Mark
Ui-Step "the root's shaft first"         { Ui-ClickPick 'edge F2.b0/E{F2:side0|F2:top} at' }
Ui-ExpectNew 'Gear Pair First Axis: 1 edge' 'the first axis picked'
Ui-ExpectNew 'Pick edge F1.b0/E{F1:side0|F1:top} at' "the motor's shaft top edge can be picked"
Ui-Mark
Ui-Step "the motor's shaft second"       { Ui-ClickPick 'edge F1.b0/E{F1:side0|F1:top} at' }
Ui-ExpectNew 'Gear Pair candidates: none: The second gear meshes with the first' "a meshing gear on another component's shaft refused with the reason" 30
Ui-Step 'Cancel (Escape)'                { Ui-Key 'Escape' }
Start-Sleep -Milliseconds 500
Ui-Mark
Ui-Step 'gear pair (F7)'                 { Ui-Key 'F7' }
Ui-ExpectNew 'Command Gear Pair started' 'the panel opened again' 30
Ui-ExpectNew 'Pick edge F1.b0/E{F1:side0|F1:top} at' "the motor's shaft top edge can be picked"
Ui-Mark
Ui-Step "the motor's shaft first"        { Ui-ClickPick 'edge F1.b0/E{F1:side0|F1:top} at' }
Ui-ExpectNew 'Gear Pair First Axis: 1 edge' 'the first axis picked'
Ui-ExpectNew 'Pick edge F2.b0/E{F2:side0|F2:top} at' "the root's shaft top edge can be picked"
Ui-Mark
Ui-Step "the root's shaft second"        { Ui-ClickPick 'edge F2.b0/E{F2:side0|F2:top} at' }
Ui-ExpectNew 'Gear Pair candidates: ' "the model's candidates" 30
Ui-ExpectNew 'center distance 60 mm' 'the axes 60 mm apart, where the occurrence puts the shaft'
Ui-Mark
Ui-Step 'operation: New Body'            { Ui-WheelLogged 'Panel Gear Pair input operation' -1 }
Ui-ExpectNew 'Gear Pair: Operation = New Body' 'as new bodies'
Ui-ExpectNew 'Preview Gear Pair: ok' 'the pair previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added gear pair, module ' 'both gears added' 30

Ui-Finish 'UI Windows drives test'
