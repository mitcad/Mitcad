# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ContinuityAnalysis.cpp app/commands/InspectCommands.cpp app/OcctViewer.cpp
# check-all sources: tools/ui-scripts/continuity-analysis.mitcad-ui tools/ui-scripts/continuity-analysis.json
# Continuity Analysis (mitcad#2521) in the build VM, as
# tools/ui-continuity-analysis-test.sh on Linux: the shared steps
# tools/ui-scripts/continuity-analysis.mitcad-ui on the rounded box
# mitcad-cli makes in the work folder (tools/ui-scripts/continuity-analysis.json).
# Inspect > Continuity Analysis with the tangent rounding's face picked
# gives its long edges G1 and its ends G0; with the body picked in the
# browser the curvature continuous rounding's edges are G2; a curvature
# tolerance of 150 % makes the tangent rounding's G2 too; Escape keeps
# nothing.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-continuity-analysis-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-continuity-analysis'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'continuity-part.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/continuity-analysis.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the rounded box was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_LOG_PICKS = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/continuity-analysis.mitcad-ui'
Ui-StartApp 'continuity-analysis' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'continuity analysis driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "continuity analysis driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Inspect Continuity Analysis: 4 edges: g2 0, g1 2, g0 2, none 0, unmeasured 0' 'the tangent rounding: G1 along, G0 at its ends'
Ui-ExpectLog 'Preview colored edges: ContinuityG1 2, ContinuityG0 2' 'its edges drawn by class'
Ui-ExpectLog 'Command Continuity Analysis cancelled' 'closed, nothing kept'
Ui-ExpectLog 'TestDriver: done' 'Continuity Analysis through the UI'
Ui-StopApp
Write-Host 'PASS: Continuity Analysis classes the edges G0, G1 and G2'
