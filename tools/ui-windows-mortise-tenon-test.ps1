# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MortiseTenonCommands.cpp core/model/src/features/mortise_tenon.rs
# check-all sources: app/framework/CommandSession.cpp tools/cli
# Input-driven Windows UI test of Mortise and Tenon (mitcad#1012) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), on two
# boards mitcad-cli makes: a rail 100 long, 20 across and 60 high (Box1
# F1, Body1, 120000 mm3) whose end face F1:side1 stands at x = -10, and a
# post 40 x 12 x 40 (Box2 F2, Body2, 19200 mm3) from x = 0 on, in front of
# the middle of the rail's end. The Linux counterpart is
# tools/ui-mortise-tenon-test.sh:
#   1. Mortise and Tenon: the end face clicked, the post picked, Width 12
#      and Length 30. OK: the rail gains 12 6 30 = 2160 mm3 of tenon, the
#      post loses 12.2 6.2 20.1 = 1520.364 mm3 of mortise; both bodies
#      changed by MortiseTenon1 in the timeline.
#   2. MortiseTenon1 edited to 25 long: both bodies built again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-mortise-tenon-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-mortise-tenon'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.mortise_tenon' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'boards.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [-110, 0], "length": 100, "width": 20, "height": 60, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 4], "length": 40, "width": 12, "height": 40, "operation": "new_body"}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'mortise-tenon' @('--open', $design)
Ui-ExpectLog 'boards.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: ' 'the boards shown'

Write-Host "--- 1. A tenon on the rail's end, its mortise in the post"
Ui-Mark
Ui-Step 'mortise and tenon (F7)'         { Ui-Key 'F7' }
Ui-ExpectNew 'Command Mortise and Tenon started' 'the panel opened' 30
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F1.b0/F1:side1 at' "the rail's end face can be picked"
Ui-Mark
Ui-Step "click the rail's end face"      { Ui-ClickPick 'face F1.b0/F1:side1 at' }
Ui-ExpectNew 'Mortise and Tenon End Face: 1 face' 'the end face picked'
Ui-Mark
Ui-Step 'pick the post'                  { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew 'Mortise and Tenon Mortise Body: 1 body [body F2.b0]' 'the post picked'
Ui-Mark
Ui-Step 'width 12'                       { Ui-TypeIn 'Panel Mortise and Tenon input width' '12' }
Ui-Step 'length 30'                      { Ui-TypeIn 'Panel Mortise and Tenon input length' '30' }
Ui-ExpectNew 'Preview Mortise and Tenon: ok' 'previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added mortise and tenon (blind)' 'the joint added' 30
Ui-ExpectVolume 'Body Body1 \(F1\.b0\): volume 120000\.000 -> ([0-9.]+) mm3' 122160 'the tenon on the rail'
Ui-ExpectVolume 'Body Body2 \(F2\.b0\): volume 19200\.000 -> ([0-9.]+) mm3' 17679.636 'the mortise in the post'
Ui-ExpectLog 'Timeline MortiseTenon1 at ' 'the joint in the timeline'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- 2. Edited to 25 long: both bodies again'
Ui-Mark
Ui-Step 'edit MortiseTenon1 (double-click)' { Ui-DoubleClickLogged 'Timeline MortiseTenon1' }
Ui-ExpectNew 'Editing F3 with Mortise and Tenon' 'the joint opens for editing' 30
Ui-Mark
Ui-Step 'length 25'                      { Ui-TypeIn 'Panel Mortise and Tenon input length' '25' }
Ui-ExpectNew 'Preview Mortise and Tenon: ok' 'the edit previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F3' 'the joint edited' 30
Ui-ExpectVolume 'Body Body1 \(F1\.b0\): volume 122160\.000 -> ([0-9.]+) mm3' 121800 'the shorter tenon'
Ui-ExpectVolume 'Body Body2 \(F2\.b0\): volume 17679\.636 -> ([0-9.]+) mm3' 18057.836 'the shallower mortise'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows mortise and tenon test'
