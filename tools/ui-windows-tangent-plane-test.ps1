# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/OcctViewer.cpp
# check-all sources: app/framework/CommandSession.cpp core/model/src/features/construction
# Input-driven Windows UI test of Tangent Plane (mitcad#446) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted):
#   1. A design (mitcad-cli): a cylinder of diameter 20 and height 30 on XY
#      about the origin (F1, its side F1:side0).
#   2. Tangent Plane, the side clicked: the reference plane is filled in
#      with an origin plane along the axis (xz or yz, not xy), the angle
#      with 0 deg (or 180 deg when that plane's normal points away from
#      the click); the preview is drawn stronger than the picked plane.
#   3. Offset 5 mm (mitcad#1779): previewed, with an arrow along the
#      normal.
#   4. OK: the plane lies 5 mm off the cylinder (its normal at right angles
#      to the axis and away from it, 15 from the axis).
#   5. Edited from the timeline: the offset comes from the feature; -3 mm
#      (into the face) is previewed and the edit made.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-tangent-plane-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-tangent-plane'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'construct.tangent_plane' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'cylinder.json'
$design = Join-Path $work 'cylinder.mitcad'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": 20,
    "height": 30, "operation": "new_body"}}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'tangent-plane' @('--open', $design)
Ui-ExpectLog 'cylinder.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800

Write-Host '--- The reference plane and the angle filled in'
Ui-Mark
Ui-Step 'tangent plane (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F1.b0/F1:side0' 'the cylinder''s side can be picked'
Ui-Step 'click the side'                 { Ui-ClickPick 'face F1.b0/F1:side0' }
Ui-ExpectNew 'Tangent Plane: reference plane ' 'a reference plane filled in'
$filled = Ui-LastMatch 'Tangent Plane: reference plane (\w+, angle .+)$'
if ($filled -notmatch '^(xz|yz),') { Ui-Fail "the reference is not an origin plane along the axis: $filled" }
if ($filled -notmatch ' (0|180) deg$') { Ui-Fail "the angle is not 0 or 180 deg: $filled" }
Write-Host "ok   reference and angle: $filled"
Ui-ExpectNew 'Preview datum drawn: transparency 0.2, picked planes 0.8' 'the preview stronger than the picked plane'

Write-Host '--- An offset of 5 mm away from the face'
Ui-Mark
Ui-Step 'offset 5'                       { Ui-TypeIn 'Panel Tangent Plane input offset' '5' }
Ui-ExpectNew 'Tangent Plane Offset: 5 = 5 mm' 'the offset typed'
Ui-ExpectNew 'Preview Tangent Plane: ok' 'the moved plane previewed' 30
Ui-ExpectLog 'Manipulator Tangent Plane offset at' 'the offset has an arrow' 30
Ui-ExpectLog 'Arrow direction Tangent Plane offset: ' 'the arrow''s direction logged' 30
$way = Ui-LastMatch 'Arrow direction Tangent Plane offset: (\S+)$'
if ($way -notmatch ',0\.000$') { Ui-Fail "the offset's arrow is not at right angles to the axis: $way" }
Write-Host "ok   the offset's arrow along the normal: $way"

Write-Host '--- The plane 5 mm off the cylinder'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added plane ' 'the plane added' 30
$added = Ui-LastMatch 'Added plane \S+: (origin \([^)]*\), normal \([^)]*\))'
$parts = $added -replace '[a-z(),]', ' ' -split ' ' | Where-Object { $_ -ne '' }
$numbers = @($parts | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) })
if ($numbers.Count -ne 6) { Ui-Fail "the plane was not logged as origin and normal: $added" }
$o = $numbers[0..2]; $n = $numbers[3..5]
if ([Math]::Abs($n[2]) -gt 1e-6) { Ui-Fail "the normal is not at right angles to the axis: $added" }
$offset = $n[0] * $o[0] + $n[1] * $o[1]
# The normal points away from the axis: the radius 10 and the offset 5.
if ([Math]::Abs($offset - 15) -gt 1e-6) { Ui-Fail "the plane is not 15 from the axis: $added" }
Write-Host "ok   the plane 5 mm off the cylinder: $added"

Write-Host '--- Edited: the offset from the feature, then into the face'
Ui-Mark
Ui-Step 'edit Plane1 (double-click)'     { Ui-DoubleClickLogged 'Timeline Plane1' }
Ui-ExpectNew ' with Tangent Plane' 'Plane1 opens for editing' 30
Ui-ExpectNew 'Tangent Plane Offset: 5 mm' 'the offset came from the feature' 30
Ui-Mark
Ui-Step 'offset -3'                      { Ui-TypeIn 'Panel Tangent Plane input offset' '-3' }
Ui-ExpectNew 'Tangent Plane Offset: -3 = -3 mm' 'into the face'
Ui-ExpectNew 'Preview Tangent Plane: ok' 'the edit previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'Plane1 edited' 30

Ui-Finish 'UI Windows tangent plane test'
