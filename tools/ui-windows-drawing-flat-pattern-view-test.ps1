# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_flat.rs
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src/compose.rs
# check-all sources: tools/ui-scripts/drawing-flat-pattern-view.mitcad-ui tools/ui-scripts/drawing-flat-pattern-view.json
# A base view of a sheet-metal body's Flat Pattern (mitcad#2280) in the
# build VM, as tools/ui-drawing-flat-pattern-view-test.sh on Linux: the
# shared steps tools/ui-scripts/drawing-flat-pattern-view.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/drawing-flat-pattern-view.json):
# Base View with Shows set to the body's Flat Pattern places its blank;
# Dimension on its bend line reads 20; the view's panel has Bend Notes,
# which it turns off.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-flat-pattern-view-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-flat-pattern-view'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plate.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-flat-pattern-view.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-flat-pattern-view.mitcad-ui'
Ui-StartApp 'drawing-flat-pattern-view' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'flat pattern view driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "flat pattern view driver failed (exit code $exitCode)"
}
Ui-ExpectLog '"type":"flat_pattern"' 'the Flat Pattern view placed from the Shows list'
Ui-ExpectLog 'Drawing annotations drawn: 1 = 20' 'the bend line dimensioned'
Ui-ExpectLog '"bend_notes":false' 'the bend notes turned off'
Ui-ExpectLog 'TestDriver: done' 'placed, dimensioned, its bend notes turned off'
Ui-StopApp
Write-Host 'PASS: drawing Flat Pattern view'
