# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/annotations.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-standard.mitcad-ui tools/ui-scripts/drawing-standard.json
# A drawing's dimensioning standard (mitcad#2047) in the build VM, as
# tools/ui-drawing-standard-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-standard.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/drawing-standard.json: an ISO drawing with a
# vertical dimension): Standard ASME in the sheet's panel writes the value
# horizontal and one Undo puts ISO back; Create Drawing's ANSI template
# sets Standard to ASME, and the choice is kept for the next time.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-standard-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-standard'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'standard.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-standard.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-standard.mitcad-ui'
Ui-StartApp 'drawing-standard' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'drawing standard driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "drawing standard driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Drawing dimension texts (asme): ' 'Standard ASME wrote the vertical value horizontal'
Ui-ExpectLog 'Create Drawing choice kept: template ansi, ANSI_B, landscape, third_angle, standard asme' 'the ANSI template brought ASME and the choice was kept'
Ui-ExpectLog 'TestDriver: done' 'the standard relaid the dimensions out, undid in one step, and Create Drawing kept it'
Ui-StopApp
Write-Host 'PASS: drawing dimensioning standard'
