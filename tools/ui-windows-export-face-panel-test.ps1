# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMake.cpp app/MainWindow.cpp app/framework/Command.hpp
# check-all sources: app/framework/CommandSession.cpp core/model-api/src/exchange.rs tools/cli
# Input-driven Windows UI test of Export Face as DXF... with nothing
# selected (mitcad#1963, GitHub mitcad/Mitcad#408) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-export-face-panel-test.sh on Linux:
#   1. A design (mitcad-cli): a 60 x 40 x 5 plate (Box1 F1, Body1).
#   2. Export Face as DXF... (the test's shortcut) with nothing selected
#      opens its panel with a Face input instead of only a hint in the
#      status bar; Escape cancels it.
#   3. Opened again, the plate's top picked in the panel and Export
#      (Enter): the file dialog, and the plate's outline written from that
#      face.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-export-face-panel-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-export-face-panel'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'make.export_face_dxf' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'plate.json'
$design = Join-Path $work 'plate.mitcad'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 60, "width": 40, "height": 5, "operation": "new_body"}}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
# The command's name ends in an ellipsis, as the log writes it.
$name = 'Export Face as DXF' + [string][char]0x2026
$dxf = Join-Path $work 'face.dxf'

Ui-StartApp 'export-face-panel' @('--open', $design)
Ui-ExpectLog 'plate.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800

Write-Host '--- Nothing selected: the panel with its Face input'
Ui-Mark
Ui-Step 'Export Face as DXF (F7)'        { Ui-Key 'F7' }
Ui-ExpectNew 'Export Face as DXF: no planar face selected, the panel opens' 'no face selected'
Ui-ExpectNew "Command $name started" 'the panel opened'
Ui-ExpectNew "Panel $name input face at" 'the panel has the Face input'
Ui-ExpectNew 'Pick face F1.b0/F1:top' 'the input takes the plate''s top'
if (@(Ui-NewLines | Where-Object { $_.Contains(': dxf outline of ') }).Count -gt 0) {
  Ui-Fail 'written without a face'
}
Ui-Step 'cancel (Escape)'                { Ui-Key 'Escape' }
Ui-ExpectNew "Command $name cancelled" 'cancelled'

Write-Host '--- The top picked in the panel and written'
Ui-Mark
Ui-Step 'Export Face as DXF (F7)'        { Ui-Key 'F7' }
Ui-ExpectNew "Command $name started" 'the panel opened again'
Ui-Step 'click the plate''s top'         { Ui-ClickPick 'face F1.b0/F1:top' }
Ui-ExpectNew "$name Face: 1 face" 'the top picked'
Ui-Step 'Export (Enter)'                 { Ui-Key 'Return' }
Ui-Step "type $dxf" {
  Ui-FocusDialog 'Export Face as DXF'
  Ui-Key 'ctrl+a'
  Ui-Type $dxf
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
  Ui-FocusMain
}
Ui-ExpectNew ': dxf outline of Body1 along F1:top' 'the plate''s outline written' 30
Ui-ExpectNew "Done $name" 'the panel closed'
if (-not (Test-Path $dxf)) { Ui-Fail 'no face DXF' }
if (-not (Select-String -Path $dxf -Pattern 'CUT_OUTER' -SimpleMatch -Quiet)) { Ui-Fail 'no outer outline in the DXF' }
Write-Host 'ok   the DXF holds the outer outline'

Ui-Finish 'UI Windows export face panel test'
