# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DogboneCommands.cpp app/commands/ModifyCommands.cpp
# check-all sources: core/model/src/features/dogbone.rs geometry/src/dogbone.cpp tools/cli
# Input-driven Windows UI test of Dogbone (mitcad#1011) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), on a design
# mitcad-cli makes: a block 80 x 60 x 20 (Box1, F1, Body1) with a pocket
# 40 x 20, 10 deep (Box2, F2): 88000 mm3. Dogbone, the block picked in the
# browser, OK with the panel's defaults (a 6.35 mm tool, 0.1 mm extra,
# diagonal): four reliefs at the pocket's inside corners, each the tool's
# circle outside the pocket (6.388881 mm2, 10 deep;
# tools/cli/tests/dogbone.json works it out), and Dogbone1 in the
# timeline. The Linux counterpart is tools/ui-dogbone-test.sh.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-dogbone-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-dogbone'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.dogbone' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'dogbone.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 80, "width": 60, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box",
    "plane": {"type": "fixed", "origin": [20, 20, 10], "normal": [0, 0, 1], "x_axis": [1, 0, 0]},
    "length": 40, "width": 20, "height": 15, "operation": "cut", "participants": ["F1.b0"]}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'dogbone' @('--open', $design)
Ui-ExpectLog 'dogbone.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the block shown'

Write-Host "--- Dogbone on the pocket's body"
Ui-Mark
Ui-Step 'dogbone (F7)'                   { Ui-Key 'F7' }
Ui-ExpectNew 'Command Dogbone started' 'the panel opened' 30
Ui-Step 'pick Body1'                     { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Dogbone Corners: 1 body [body F1.b0]' 'the block picked' 30
Ui-ExpectNew 'Preview Dogbone: ok' 'the reliefs previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added dogbone (Diagonal), tool diameter 6.35' 'the dogbone added' 30
Ui-ExpectVolume 'Body Body1 \(F1\.b0\): volume 88000\.000 -> ([0-9.]+) mm3' 87744.444755 'four reliefs cut'
Ui-ExpectLog 'Timeline Dogbone1 at' 'Dogbone1 in the timeline'
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows dogbone test'
