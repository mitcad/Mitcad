# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/RememberedValues.cpp tools/cli
# Input-driven Windows UI test of the remembered values of Fillet and
# Chamfer (mitcad#1028) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), on a design mitcad-cli makes: a block 60 x 40 x 20
# (Box1, F1, Body1). The test library starts panels from their defaults
# (MITCAD_REMEMBER_PANEL_VALUES=0); this test turns the remembered values
# on, in settings of its own:
#   - Fillet starts at 2 mm; the top face's edges filleted with 3 mm
#     typed, OK; Fillet again starts with 3 mm (logged "Panel Fillet
#     radius: remembered 3 mm"); 5 mm typed and cancelled changes nothing.
#   - The fillet undone; Chamfer starts at 1 mm; the top face's edges
#     bevelled with 3 mm typed, OK; Chamfer again starts with 3 mm.
#   - A new start of the app: Fillet still starts with 3 mm.
# The Linux counterparts are the remembered steps of
# tools/ui-fillet-handle-test.sh and tools/ui-chamfer-handle-test.sh.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-remembered-values-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-remembered-values'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.chamfer' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'remembered.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 60, "width": 40, "height": 20, "operation": "new_body"}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Remembered values on for this test (the library leaves a set value).
$env:MITCAD_REMEMBER_PANEL_VALUES = '1'
Ui-StartApp 'remembered' @('--open', $design)
Ui-ExpectLog 'remembered.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the block shown'

Write-Host '--- Fillet with 3 mm'
Ui-Mark
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectNew 'Fillet Radius: 2 mm = 2 mm' 'nothing remembered yet: the default 2 mm' 30
if ((Ui-LogLines) | Where-Object { $_.Contains('Panel Fillet radius: remembered') }) {
  Ui-Fail 'a radius remembered before any OK'
}
Ui-Step 'pick the top face'              { Ui-ClickPick 'face F1.b0/F1:top' }
Ui-ExpectNew 'Fillet Edges: 1 face' 'the top face picked'
Ui-Step 'radius 3'                       { Ui-TypeIn 'Panel Fillet input sets.0.radius' '3' }
Ui-ExpectNew 'Fillet Radius: 3 = 3 mm' '3 mm typed'
Ui-ExpectNew 'Preview Fillet: ok' 'previewed' 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added fillet on 4 edge(s)' "fillet on the face's 4 edges" 30
Ui-WaitIdle

Write-Host '--- Fillet again starts with 3 mm; a cancelled 5 mm changes nothing'
Ui-Mark
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectNew 'Panel Fillet radius: remembered 3 mm' 'the radius of the last OK remembered' 30
Ui-ExpectNew 'Fillet Radius: 3 mm = 3 mm' "the panel's Radius is 3 mm"
Ui-Step 'radius 5'                       { Ui-TypeIn 'Panel Fillet input sets.0.radius' '5' }
Ui-ExpectNew 'Fillet Radius: 5 = 5 mm' '5 mm typed'
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Fillet cancelled' 'the fillet cancelled' 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'undo the fillet (Ctrl+Z)'       { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Add Fillet1' 'back to the block' 30
Ui-WaitIdle

Write-Host '--- Chamfer with 3 mm'
Ui-Mark
Ui-Step 'chamfer (F7)'                   { Ui-Key 'F7' }
Ui-ExpectNew 'Chamfer Distance: 1 mm = 1 mm' 'nothing remembered yet: the default 1 mm' 30
Ui-Step 'pick the top face'              { Ui-ClickPick 'face F1.b0/F1:top' }
Ui-ExpectNew 'Chamfer Edges: 1 face' 'the top face picked'
Ui-Step 'distance 3'                     { Ui-TypeIn 'Panel Chamfer input sets.0.distance' '3' }
Ui-ExpectNew 'Chamfer Distance: 3 = 3 mm' '3 mm typed'
Ui-ExpectNew 'Preview Chamfer: ok' 'previewed' 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added chamfer (Equal Distance) on 4 edge(s)' "chamfer on the face's 4 edges" 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'chamfer (F7)'                   { Ui-Key 'F7' }
Ui-ExpectNew 'Panel Chamfer distance: remembered 3 mm' 'the distance of the last OK remembered' 30
Ui-ExpectNew 'Chamfer Distance: 3 mm = 3 mm' "the panel's Distance is 3 mm"
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Chamfer cancelled' 'the chamfer cancelled' 30
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-StopApp

Write-Host '--- A new start of the app: Fillet still starts with 3 mm'
Ui-StartApp 'again' @('--open', $design)
Ui-ExpectLog 'remembered.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectNew 'Panel Fillet radius: remembered 3 mm' 'the cancelled 5 mm did not change it' 30
Ui-ExpectNew 'Fillet Radius: 3 mm = 3 mm' "the panel's Radius is 3 mm"
Remove-Item Env:MITCAD_REMEMBER_PANEL_VALUES -ErrorAction SilentlyContinue

Ui-Finish 'UI Windows remembered values test'
