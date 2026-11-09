# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_lines.rs core/model-drawing/src/compose.rs
# check-all sources: tools/ui-scripts/drawing-edge-line.mitcad-ui tools/ui-scripts/drawing-edge-line.json
# Edit Line on a projected edge (mitcad#1546) in the build VM, as
# tools/ui-drawing-edge-line-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-edge-line.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/drawing-edge-line.json): a right click
# on the step's edge in its top view offers Edit Line..., whose Thick and
# Long Dash Double Dot make that edge's line wide and long dash double
# dotted in the view, and Reset to View Style takes the choice away.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-edge-line-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-edge-line'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'step.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-edge-line.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-edge-line.mitcad-ui'
Ui-StartApp 'drawing-edge-line' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'Edit Line driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Edit Line driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Drawing line dialog opened: edge ' 'Edit Line opened on the edge under the pointer'
Ui-ExpectLog 'Drawing view pens: ' 'the sheet logged its pens'
Ui-ExpectLog 'TestDriver: done' 'Edit Line set the edge line width and pattern and reset them'
Ui-StopApp
Write-Host 'PASS: drawing Edit Line on a projected edge'
