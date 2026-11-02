# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp core/model-api/src/analysis.rs tools/cli
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-measure-planar'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'stack.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'cli/tests/measure_planar.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'planar measure fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/measure-planar.mitcad-ui'
Ui-StartApp 'measure-planar' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'planar measure driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "planar measure driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'plane gap, separate minimum and perpendicular dimension'
Ui-StopApp
Write-Host 'PASS: planar Measure'
