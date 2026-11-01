# SPDX-License-Identifier: MIT
# check-all sources: app/commands/GearCommands.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/gear.rs core/model/src/features/gear_profile.rs geometry/src/gear.cpp
# check-all sources: app/commands/PlaceInputs.cpp core/model/src/features/seat.rs core/model-api/src/seats.rs tools/cli
# Input-driven Windows UI test of Gear (mitcad#472) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted); the Linux
# counterpart is tools/ui-gear-test.sh. The Backlash and the Bore Clearance
# start as the printed-fit parameters gear_backlash and shaft_bore_clearance
# (0.2 mm each, mitcad#623):
#   1. Gear in an empty design, placed by a click on the XY plane and both
#      distances 0 (it waits for a pick, mitcad#656): a spur gear of 20
#      teeth, module 2, 10 wide; its note shows the derived diameters
#      (pitch 40, outside 44, root 35). OK makes Gear1 (F1.b0) of the
#      section's area times the width, its teeth thinner by the backlash
#      (12283.094 mm3).
#   2. Gear again: Rack picked by its icon, Mesh With a face of Gear1 (the
#      mesh places a rack, no pick needed), 10 teeth, 12 high: the note shows its pitch line (20 pi = 62.8319 mm) and
#      the centre distance (20 mm); OK makes the rack below the gear
#      (F2.b0, its section's area times the width: 6173.320 mm3).
#   3. On a shaft (mitcad#578), in a design mitcad-cli makes (an 8 mm
#      cylinder 30 high, Cylinder1, F1.b0): Gear (Spur, 20 teeth) with
#      Place On the cylinder's top edge: the note shows the bore the shaft
#      gives with the printed fit (8.2 mm); an offset of 5 mm; OK makes the
#      gear on the shaft (F2.b0: Gear1's volume less the 8.2 mm bore
#      through its 10 mm, 12283.094 - 528.102 = 11754.992 mm3).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-gear-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-gear'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.gear' = 'F7' } -StepPause 0

# The shaft of part 3, made by mitcad-cli.
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$shaftJson = Join-Path $work 'shaft.json'
Set-Content -Encoding ascii $shaftJson @'
[
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}}
]
'@
$shaft = Join-Path $work 'shaft.mitcad'
$output = & $cli run $shaftJson --save $shaft 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

Ui-StartApp 'gear'
Start-Sleep -Milliseconds 800

Write-Host '--- A spur gear on XY'
Ui-Mark
Ui-Step 'gear (F7)'                      { Ui-Key 'F7' }
Ui-PlaceOnXy 'Gear'
Ui-ExpectNew 'Panel Gear note: Pitch diameter 40 mm, outside 44 mm, root 35 mm' 'the derived diameters shown'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added gear (Spur), 20 teeth, module 2' 'the gear added' 30
Ui-ExpectVolume 'New body \S+ \(F1\.b0\): volume ([0-9.]+) mm3' 12283.094 'Gear1' 1e-5

Write-Host '--- A rack meshing it'
Ui-Mark
Ui-Step 'gear (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Gear started' 'the panel opened, waiting for a pick' 30
Ui-Step 'Rack'                           { Ui-ClickLogged 'Panel Gear input kind=rack' }
Ui-Step 'teeth 10'                       { Ui-TypeIn 'Panel Gear input teeth' '10' }
Ui-Step 'height 12'                      { Ui-TypeIn 'Panel Gear input height' '12' }
Ui-Mark
Ui-Step 'Mesh With'                      { Ui-ClickLogged 'Panel Gear input mesh_with' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F1.b0/F1:' 'a face of Gear1 can be picked'
$face = Ui-LastMatch 'Pick face F1\.b0/(F1:[a-z_]+\(\d+\)) at'
Ui-Mark
Ui-Step "click $face"                    { Ui-ClickPick "face F1.b0/$face at" }
Ui-ExpectNew 'Panel Gear note: Pitch line 62.8319 mm, center distance 20 mm' 'the rack''s pitch line and centre distance shown' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added gear (Rack), 10 teeth, module 2' 'the rack added' 30
Ui-ExpectVolume 'New body \S+ \(F2\.b0\): volume ([0-9.]+) mm3' 6173.320 'the rack' 1e-5

Write-Host '--- A spur gear on a shaft (mitcad#578)'
Ui-StartApp 'gear-shaft' @('--open', $shaft)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the shaft shown'
Ui-Mark
Ui-Step 'gear (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Gear started' 'the panel opened, waiting for a pick' 30
# The panel may keep the rack of part 2: a spur gear of 20 teeth again.
Ui-Step 'Spur'                           { Ui-ClickLogged 'Panel Gear input kind=spur' }
Ui-Step 'teeth 20'                       { Ui-TypeIn 'Panel Gear input teeth' '20' }
Ui-Mark
Ui-Step 'Place On'                       { Ui-ClickLogged 'Panel Gear input at' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick edge F1.b0/E{F1:side0|F1:top} at' "the shaft's top edge can be picked"
Ui-Mark
Ui-Step "click the shaft's top edge"     { Ui-ClickPick 'edge F1.b0/E{F1:side0|F1:top} at' }
Ui-ExpectNew 'Place Gear: axis edge E{F1:side0|F1:top} of F1.b0' "placed on the shaft's end"
Ui-ExpectNew 'bore 8.2 mm (shaft 8 mm)' 'the bore the shaft gives, with the printed fit' 30
Ui-Mark
Ui-Step 'More'                           { Ui-ClickLogged 'Panel Gear input at_more' }
Ui-Step 'offset 5'                       { Ui-TypeIn 'Panel Gear input at_offset' '5' }
Ui-ExpectNew 'Preview Gear: ok' '5 mm down the shaft previewed' 30
$note = Ui-LastMatch '(Panel Gear note: .*)'
Write-Host "     $note"
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added gear (Spur), 20 teeth, module 2' 'the gear on the shaft added' 30
Ui-ExpectVolume 'New body \S+ \(F2\.b0\): volume ([0-9.]+) mm3' 11754.992 'the gear with the shaft''s bore' 1e-5

Ui-Finish 'UI Windows gear test'
