# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-concentric-dimensions.mitcad-ui tools/ui-scripts/sketch-concentric-dimensions.json
# The values of two concentric circles' diameters keep apart (mitcad#1673)
# on Windows, the shared input script
# tools/ui-scripts/sketch-concentric-dimensions.mitcad-ui as
# tools/ui-sketch-concentric-dimensions-test.sh runs it on Linux: Circle at
# (5, 3) typed 5.2, then Circle at the same centre typed 5; the two values'
# boxes do not overlap after the fit, zoomed in, and zoomed out far.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-concentric-dimensions-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-concentric-dimensions'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'concentric.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-concentric-dimensions.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the sketch concentric dimensions design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-concentric-dimensions.mitcad-ui'
Ui-StartApp 'sketch-concentric-dimensions' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch concentric dimensions driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch concentric dimensions driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added circle, diameter 5.2 mm at (5, 3)' 'the outer circle typed 5.2'
Ui-ExpectLog 'Added circle, diameter 5 mm at (5, 3)' 'the inner circle typed 5 round the same centre'
Ui-ExpectLog 'Sketch dimension values apart: 2 values' 'the two diameter values apart'
Ui-ExpectLog 'TestDriver: done' 'the values apart after the fit, zoomed in and zoomed out'
Ui-StopApp
Write-Host 'PASS: sketch concentric circle dimensions apart'
